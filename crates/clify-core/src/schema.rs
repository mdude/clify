//! JSON Schema generation from Rust types.
//!
//! Generates `clify-spec.schema.json` for IDE autocomplete and CI validation.

use crate::spec::ClifySpec;
use schemars::schema_for;

/// Generate JSON Schema for the `.clify.yaml` spec format.
pub fn generate_json_schema() -> String {
    let schema = schema_for!(ClifySpec);
    serde_json::to_string_pretty(&schema).expect("Failed to serialize JSON Schema")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_generates_valid_json() {
        let schema = generate_json_schema();
        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&schema).expect("Schema is not valid JSON");
        // Should have a definitions or $defs section
        assert!(parsed.get("$schema").is_some() || parsed.get("definitions").is_some() || parsed.get("title").is_some());
    }

    #[test]
    fn schema_references_key_types() {
        let schema = generate_json_schema();
        // Should reference our main types
        assert!(schema.contains("ClifySpec") || schema.contains("meta"));
        assert!(schema.contains("transport") || schema.contains("Transport"));
        assert!(schema.contains("commands") || schema.contains("Command"));
    }
}
