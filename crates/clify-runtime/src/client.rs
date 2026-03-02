//! HTTP client — request building, retries, pagination.

use crate::auth::{AuthManager, ResolvedAuth};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("HTTP {status}: {body}")]
    HttpError { status: u16, body: String },
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("Auth error: {0}")]
    AuthError(#[from] crate::auth::AuthError),
    #[error("API error: {0}")]
    ApiError(String),
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub timeout_secs: u64,
    pub retries: u32,
    pub default_headers: HashMap<String, String>,
}

pub struct ApiClient {
    config: ClientConfig,
    http: reqwest::Client,
}

impl ApiClient {
    pub fn new(config: ClientConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");
        Self { config, http }
    }

    /// Execute an API request with auth, retries, and error handling.
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        query_params: &HashMap<String, String>,
        body_params: &HashMap<String, serde_json::Value>,
        header_params: &HashMap<String, String>,
        content_type: &str,
        auth: Option<&ResolvedAuth>,
        success_status: &[u16],
        error_path: Option<&str>,
    ) -> Result<serde_json::Value, ClientError> {
        let url = format!("{}{}", self.config.base_url.trim_end_matches('/'), path);

        let mut last_err = None;
        let max_attempts = self.config.retries + 1;

        for attempt in 0..max_attempts {
            if attempt > 0 {
                let delay = std::time::Duration::from_millis(1000 * 2u64.pow(attempt - 1));
                tokio::time::sleep(delay).await;
            }

            match self.do_request(
                method, &url, query_params, body_params, header_params,
                content_type, auth, success_status, error_path,
            ).await {
                Ok(val) => return Ok(val),
                Err(e) => {
                    // Only retry on 5xx or network errors
                    match &e {
                        ClientError::HttpError { status, .. } if *status >= 500 => {
                            last_err = Some(e);
                            continue;
                        }
                        ClientError::RequestFailed(_) => {
                            last_err = Some(e);
                            continue;
                        }
                        _ => return Err(e),
                    }
                }
            }
        }

        Err(last_err.unwrap())
    }

    async fn do_request(
        &self,
        method: &str,
        url: &str,
        query_params: &HashMap<String, String>,
        body_params: &HashMap<String, serde_json::Value>,
        header_params: &HashMap<String, String>,
        content_type: &str,
        auth: Option<&ResolvedAuth>,
        success_status: &[u16],
        error_path: Option<&str>,
    ) -> Result<serde_json::Value, ClientError> {
        let method = method.parse::<reqwest::Method>()
            .map_err(|e| ClientError::RequestFailed(e.to_string()))?;

        let mut builder = self.http.request(method, url);

        // Default headers
        for (k, v) in &self.config.default_headers {
            builder = builder.header(k.as_str(), v.as_str());
        }

        // Per-request headers
        for (k, v) in header_params {
            builder = builder.header(k.as_str(), v.as_str());
        }

        // Query params
        if !query_params.is_empty() {
            builder = builder.query(query_params);
        }

        // Body
        if !body_params.is_empty() {
            match content_type {
                "json" => {
                    builder = builder.json(body_params);
                }
                "form" => {
                    // Convert Values to strings for form encoding
                    let form_data: HashMap<String, String> = body_params.iter()
                        .map(|(k, v)| (k.clone(), value_to_string(v)))
                        .collect();
                    builder = builder.form(&form_data);
                }
                _ => {
                    builder = builder.json(body_params);
                }
            }
        }

        // Auth
        if let Some(auth) = auth {
            builder = auth.apply(builder);
        }

        let resp = builder.send().await
            .map_err(|e| ClientError::RequestFailed(e.to_string()))?;

        let status = resp.status().as_u16();
        let body = resp.text().await
            .map_err(|e| ClientError::RequestFailed(e.to_string()))?;

        // Parse response
        let json: serde_json::Value = serde_json::from_str(&body)
            .unwrap_or_else(|_| serde_json::Value::String(body.clone()));

        // Check for API-level errors (some APIs return 200 with error in body)
        if let Some(err_path) = error_path {
            if let Some(err_msg) = extract_path(&json, err_path) {
                if let Some(msg) = err_msg.as_str() {
                    if !msg.is_empty() {
                        return Err(ClientError::ApiError(msg.to_string()));
                    }
                }
            }
        }

        // Check HTTP status
        let expected = if success_status.is_empty() { &[200u16][..] } else { success_status };
        if !expected.contains(&status) {
            return Err(ClientError::HttpError { status, body });
        }

        Ok(json)
    }

    /// Paginate through all results.
    pub async fn paginate(
        &self,
        method: &str,
        path: &str,
        query_params: &mut HashMap<String, String>,
        body_params: &HashMap<String, serde_json::Value>,
        header_params: &HashMap<String, String>,
        content_type: &str,
        auth: Option<&ResolvedAuth>,
        success_status: &[u16],
        error_path: Option<&str>,
        success_path: Option<&str>,
        pagination: &PaginationConfig,
        max_results: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, ClientError> {
        let mut all_results = Vec::new();
        let mut offset = 0u64;
        let page_size = pagination.default_page_size.unwrap_or(100);

        loop {
            // Set pagination params
            match pagination.pagination_type.as_str() {
                "offset" => {
                    query_params.insert(pagination.param.clone(), offset.to_string());
                    if let Some(ref ps_param) = pagination.page_size_param {
                        query_params.insert(ps_param.clone(), page_size.to_string());
                    }
                }
                "cursor" => {
                    if offset > 0 {
                        // cursor is set from previous response
                    }
                    if let Some(ref ps_param) = pagination.page_size_param {
                        query_params.insert(ps_param.clone(), page_size.to_string());
                    }
                }
                _ => {}
            }

            let resp = self.request(
                method, path, query_params, body_params, header_params,
                content_type, auth, success_status, error_path,
            ).await?;

            // Extract results
            let page_data = if let Some(sp) = success_path {
                extract_path(&resp, sp).cloned().unwrap_or(serde_json::Value::Array(vec![]))
            } else {
                resp.clone()
            };

            let items = match page_data {
                serde_json::Value::Array(arr) => arr,
                other => vec![other],
            };

            let items_count = items.len();
            all_results.extend(items);

            // Check max_results
            if let Some(max) = max_results {
                if all_results.len() >= max {
                    all_results.truncate(max);
                    break;
                }
            }

            // Check if done
            if items_count == 0 || items_count < page_size as usize {
                break;
            }

            match pagination.pagination_type.as_str() {
                "offset" => {
                    offset += page_size as u64;
                }
                "cursor" => {
                    if let Some(ref next_path) = pagination.next_path {
                        if let Some(next) = extract_path(&resp, next_path) {
                            if let Some(cursor) = next.as_str() {
                                query_params.insert(pagination.param.clone(), cursor.to_string());
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        Ok(all_results)
    }
}

#[derive(Debug, Clone)]
pub struct PaginationConfig {
    pub pagination_type: String,
    pub param: String,
    pub page_size_param: Option<String>,
    pub default_page_size: Option<u32>,
    pub next_path: Option<String>,
    pub total_path: Option<String>,
}

/// Extract a value from JSON using dot-notation path (e.g., "error.message", "results[0]").
pub fn extract_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for part in path.split('.') {
        // Handle array indexing: "results[0]"
        if let Some(bracket_pos) = part.find('[') {
            let key = &part[..bracket_pos];
            let idx_str = &part[bracket_pos + 1..part.len() - 1];
            if !key.is_empty() {
                current = current.get(key)?;
            }
            let idx: usize = idx_str.parse().ok()?;
            current = current.get(idx)?;
        } else {
            current = current.get(part)?;
        }
    }
    Some(current)
}

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_path_simple() {
        let json: serde_json::Value = serde_json::json!({"error": {"message": "not found"}});
        let result = extract_path(&json, "error.message");
        assert_eq!(result.unwrap().as_str().unwrap(), "not found");
    }

    #[test]
    fn test_extract_path_array() {
        let json: serde_json::Value = serde_json::json!({"results": [{"name": "a"}, {"name": "b"}]});
        let result = extract_path(&json, "results[1].name");
        assert_eq!(result.unwrap().as_str().unwrap(), "b");
    }

    #[test]
    fn test_extract_path_missing() {
        let json: serde_json::Value = serde_json::json!({"data": 1});
        assert!(extract_path(&json, "missing.path").is_none());
    }
}
