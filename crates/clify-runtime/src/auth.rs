//! Authentication management — token storage, OAuth2, API keys.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("No credentials found. Run `{cli_name} auth login` first.")]
    NoCredentials { cli_name: String },
    #[error("Token expired and refresh failed: {0}")]
    TokenExpired(String),
    #[error("Auth request failed: {0}")]
    RequestFailed(String),
    #[error("Failed to read/write auth storage: {0}")]
    StorageError(String),
    #[error("Missing environment variable: {0}")]
    MissingEnvVar(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCredentials {
    pub token: String,
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AuthStrategy {
    None,
    ApiKey {
        location: ApiKeyLocation,
        name: String,
        env: String,
    },
    Token {
        env: String,
    },
    Basic {
        env_user: String,
        env_pass: String,
    },
    Oauth2 {
        grant: String,
        token_url: String,
        env_client_id: String,
        env_client_secret: String,
        custom: Option<Oauth2Custom>,
    },
}

#[derive(Debug, Clone)]
pub enum ApiKeyLocation {
    Header,
    Query,
}

#[derive(Debug, Clone)]
pub struct Oauth2Custom {
    pub token_field: String,
    pub expiry_field: String,
    pub content_type: String,
    pub extra_params: HashMap<String, String>,
}

pub struct AuthManager {
    strategy: AuthStrategy,
    storage_path: PathBuf,
    cli_name: String,
}

impl AuthManager {
    pub fn new(strategy: AuthStrategy, cli_name: &str) -> Self {
        let storage_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(cli_name)
            .join("auth.json");
        Self {
            strategy,
            storage_path,
            cli_name: cli_name.to_string(),
        }
    }

    /// Get the current token/credentials, resolving from flags > env > storage.
    pub fn resolve_token(&self, explicit_token: Option<&str>) -> Result<Option<ResolvedAuth>, AuthError> {
        // 1. Explicit flag
        if let Some(token) = explicit_token {
            return Ok(Some(ResolvedAuth::Bearer(token.to_string())));
        }

        match &self.strategy {
            AuthStrategy::None => Ok(None),

            AuthStrategy::ApiKey { location, name, env } => {
                let key = self.env_or_stored(env, "api_key")?;
                Ok(Some(ResolvedAuth::ApiKey {
                    location: location.clone(),
                    name: name.clone(),
                    value: key,
                }))
            }

            AuthStrategy::Token { env } => {
                let token = self.env_or_stored(env, "token")?;
                Ok(Some(ResolvedAuth::Bearer(token)))
            }

            AuthStrategy::Basic { env_user, env_pass } => {
                let user = std::env::var(env_user)
                    .map_err(|_| AuthError::MissingEnvVar(env_user.clone()))?;
                let pass = std::env::var(env_pass)
                    .map_err(|_| AuthError::MissingEnvVar(env_pass.clone()))?;
                Ok(Some(ResolvedAuth::Basic { user, pass }))
            }

            AuthStrategy::Oauth2 { .. } => {
                // Try stored token first
                if let Ok(creds) = self.load_stored() {
                    if !self.is_expired(&creds) {
                        return Ok(Some(ResolvedAuth::Bearer(creds.token)));
                    }
                }
                // Token expired or not found — need to fetch
                Err(AuthError::NoCredentials { cli_name: self.cli_name.clone() })
            }
        }
    }

    /// Perform OAuth2 token fetch (client_credentials).
    pub async fn oauth2_login(&self) -> Result<StoredCredentials, AuthError> {
        let (token_url, client_id, client_secret, custom) = match &self.strategy {
            AuthStrategy::Oauth2 {
                token_url,
                env_client_id,
                env_client_secret,
                custom,
                ..
            } => {
                let id = std::env::var(env_client_id)
                    .map_err(|_| AuthError::MissingEnvVar(env_client_id.clone()))?;
                let secret = std::env::var(env_client_secret)
                    .map_err(|_| AuthError::MissingEnvVar(env_client_secret.clone()))?;
                (token_url.clone(), id, secret, custom.clone())
            }
            _ => return Err(AuthError::RequestFailed("Not an OAuth2 strategy".to_string())),
        };

        let client = reqwest::Client::new();
        let mut params = HashMap::new();
        params.insert("grant_type", "client_credentials".to_string());
        params.insert("client_id", client_id);
        params.insert("client_secret", client_secret);

        // Add extra params from custom config
        let extra = custom.as_ref().map(|c| c.extra_params.clone()).unwrap_or_default();
        for (k, v) in &extra {
            params.insert(k.as_str(), v.clone());
        }

        let resp = client
            .post(&token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AuthError::RequestFailed(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AuthError::RequestFailed(format!("HTTP {}: {}", 0, body)));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AuthError::RequestFailed(e.to_string()))?;

        let token_field = custom.as_ref()
            .map(|c| c.token_field.as_str())
            .unwrap_or("access_token");
        let expiry_field = custom.as_ref()
            .map(|c| c.expiry_field.as_str())
            .unwrap_or("expires_in");

        let token = body.get(token_field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| AuthError::RequestFailed(format!("Missing '{}' in response", token_field)))?
            .to_string();

        let expires_in = body.get(expiry_field)
            .and_then(|v| v.as_u64());

        let expires_at = expires_in.map(|ei| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() + ei
        });

        let creds = StoredCredentials {
            token,
            expires_at,
            refresh_token: None,
            token_type: Some("Bearer".to_string()),
        };

        self.save_stored(&creds)?;
        Ok(creds)
    }

    /// Interactive login — prompts or performs OAuth flow.
    pub async fn login(&self) -> Result<(), AuthError> {
        match &self.strategy {
            AuthStrategy::None => {
                println!("No authentication configured.");
                Ok(())
            }
            AuthStrategy::ApiKey { env, .. } | AuthStrategy::Token { env } => {
                println!("Enter your token/API key (or set {} env var):", env);
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)
                    .map_err(|e| AuthError::StorageError(e.to_string()))?;
                let token = input.trim().to_string();
                let creds = StoredCredentials {
                    token,
                    expires_at: None,
                    refresh_token: None,
                    token_type: None,
                };
                self.save_stored(&creds)?;
                println!("✓ Credentials saved.");
                Ok(())
            }
            AuthStrategy::Basic { .. } => {
                println!("Set env vars for basic auth, or use --user/--pass flags.");
                Ok(())
            }
            AuthStrategy::Oauth2 { .. } => {
                let creds = self.oauth2_login().await?;
                println!("✓ Authenticated. Token expires at {:?}", creds.expires_at);
                Ok(())
            }
        }
    }

    pub fn status(&self) -> String {
        match self.load_stored() {
            Ok(creds) => {
                if self.is_expired(&creds) {
                    "⚠ Token expired".to_string()
                } else {
                    let preview = if creds.token.len() > 12 {
                        format!("{}...{}", &creds.token[..6], &creds.token[creds.token.len()-4..])
                    } else {
                        "****".to_string()
                    };
                    format!("✓ Authenticated (token: {})", preview)
                }
            }
            Err(_) => "✗ Not authenticated".to_string(),
        }
    }

    pub fn logout(&self) -> Result<(), AuthError> {
        if self.storage_path.exists() {
            std::fs::remove_file(&self.storage_path)
                .map_err(|e| AuthError::StorageError(e.to_string()))?;
        }
        Ok(())
    }

    fn env_or_stored(&self, env_var: &str, field: &str) -> Result<String, AuthError> {
        // 2. Env var
        if let Ok(val) = std::env::var(env_var) {
            return Ok(val);
        }
        // 3. Stored
        if let Ok(creds) = self.load_stored() {
            return Ok(creds.token);
        }
        Err(AuthError::NoCredentials { cli_name: self.cli_name.clone() })
    }

    fn load_stored(&self) -> Result<StoredCredentials, AuthError> {
        let data = std::fs::read_to_string(&self.storage_path)
            .map_err(|e| AuthError::StorageError(e.to_string()))?;
        serde_json::from_str(&data)
            .map_err(|e| AuthError::StorageError(e.to_string()))
    }

    fn save_stored(&self, creds: &StoredCredentials) -> Result<(), AuthError> {
        if let Some(parent) = self.storage_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AuthError::StorageError(e.to_string()))?;
        }
        let data = serde_json::to_string_pretty(creds)
            .map_err(|e| AuthError::StorageError(e.to_string()))?;
        std::fs::write(&self.storage_path, data)
            .map_err(|e| AuthError::StorageError(e.to_string()))
    }

    fn is_expired(&self, creds: &StoredCredentials) -> bool {
        if let Some(expires_at) = creds.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now >= expires_at
        } else {
            false
        }
    }
}

/// Resolved authentication ready to apply to a request.
#[derive(Debug, Clone)]
pub enum ResolvedAuth {
    Bearer(String),
    ApiKey {
        location: ApiKeyLocation,
        name: String,
        value: String,
    },
    Basic {
        user: String,
        pass: String,
    },
}

impl ResolvedAuth {
    /// Apply auth to a reqwest::RequestBuilder.
    pub fn apply(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            ResolvedAuth::Bearer(token) => builder.bearer_auth(token),
            ResolvedAuth::ApiKey { location, name, value } => {
                match location {
                    ApiKeyLocation::Header => builder.header(name.as_str(), value.as_str()),
                    ApiKeyLocation::Query => builder.query(&[(name.as_str(), value.as_str())]),
                }
            }
            ResolvedAuth::Basic { user, pass } => builder.basic_auth(user, Some(pass)),
        }
    }
}
