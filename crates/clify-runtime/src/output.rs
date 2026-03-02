//! Output formatting — JSON, table, CSV.

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("Failed to format output: {0}")]
    FormatError(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Format {
    Json,
    Table,
    Csv,
}

impl Format {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "table" => Format::Table,
            "csv" => Format::Csv,
            _ => Format::Json,
        }
    }
}

pub struct OutputFormatter {
    pub format: Format,
    pub pretty: bool,
}

impl OutputFormatter {
    pub fn new(format: Format, pretty: bool) -> Self {
        Self { format, pretty }
    }

    /// Format and print the value. If success_path is given, extract that first.
    pub fn print(&self, value: &Value, success_path: Option<&str>) -> Result<(), OutputError> {
        let data = if let Some(path) = success_path {
            crate::client::extract_path(value, path)
                .cloned()
                .unwrap_or_else(|| value.clone())
        } else {
            value.clone()
        };

        match self.format {
            Format::Json => self.print_json(&data),
            Format::Table => self.print_table(&data),
            Format::Csv => self.print_csv(&data),
        }
    }

    fn print_json(&self, value: &Value) -> Result<(), OutputError> {
        let output = if self.pretty {
            serde_json::to_string_pretty(value)
        } else {
            serde_json::to_string(value)
        }.map_err(|e| OutputError::FormatError(e.to_string()))?;
        println!("{}", output);
        Ok(())
    }

    fn print_table(&self, value: &Value) -> Result<(), OutputError> {
        let rows = match value {
            Value::Array(arr) => arr.clone(),
            obj @ Value::Object(_) => vec![obj.clone()],
            _ => {
                println!("{}", value);
                return Ok(());
            }
        };

        if rows.is_empty() {
            println!("(no results)");
            return Ok(());
        }

        // Collect all keys from all rows for headers
        let mut headers = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for row in &rows {
            if let Value::Object(map) = row {
                for key in map.keys() {
                    if seen.insert(key.clone()) {
                        headers.push(key.clone());
                    }
                }
            }
        }

        let mut table = comfy_table::Table::new();
        table.set_header(&headers);
        table.load_preset(comfy_table::presets::UTF8_FULL_CONDENSED);

        for row in &rows {
            let cells: Vec<String> = headers.iter().map(|h| {
                row.get(h)
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        Value::Null => "".to_string(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
            }).collect();
            table.add_row(cells);
        }

        println!("{}", table);
        Ok(())
    }

    fn print_csv(&self, value: &Value) -> Result<(), OutputError> {
        let rows = match value {
            Value::Array(arr) => arr.clone(),
            obj @ Value::Object(_) => vec![obj.clone()],
            _ => {
                println!("{}", value);
                return Ok(());
            }
        };

        if rows.is_empty() {
            return Ok(());
        }

        // Collect headers
        let mut headers = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for row in &rows {
            if let Value::Object(map) = row {
                for key in map.keys() {
                    if seen.insert(key.clone()) {
                        headers.push(key.clone());
                    }
                }
            }
        }

        let mut wtr = csv::Writer::from_writer(std::io::stdout());
        wtr.write_record(&headers).map_err(|e| OutputError::FormatError(e.to_string()))?;

        for row in &rows {
            let record: Vec<String> = headers.iter().map(|h| {
                row.get(h)
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        Value::Null => "".to_string(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
            }).collect();
            wtr.write_record(&record).map_err(|e| OutputError::FormatError(e.to_string()))?;
        }

        wtr.flush().map_err(|e| OutputError::FormatError(e.to_string()))?;
        Ok(())
    }
}
