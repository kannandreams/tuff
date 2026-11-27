//! Host tables: `[x.<namespace>]` in `tuff-pack.toml` and `tuff.toml`
//! (RFC-110 D6).
//!
//! An application that installs packs for its own users can keep its own
//! settings in a pack, under a namespace it chooses. Tuff checks only the
//! shape, carries the tables unchanged, and returns them in JSON output. It
//! never reads a key inside one or acts on it.

use std::collections::BTreeMap;

use crate::error::{Result, TuffError};

/// Host tables keyed by namespace, as written in TOML.
pub type HostTables = BTreeMap<String, toml::Table>;

/// Host tables as JSON values, the form JSON output and artifact metadata
/// carry them in.
pub type HostTablesJson = BTreeMap<String, serde_json::Value>;

/// Check that every namespace is a plain name. The tables themselves are
/// tables by construction, since `x` deserializes as a table of tables.
///
/// # Errors
///
/// Returns a usage error naming `place` and the offending namespace.
pub fn validate(tables: &HostTables, place: &str) -> Result<()> {
    for namespace in tables.keys() {
        let plain = !namespace.is_empty()
            && namespace
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !plain {
            return Err(TuffError::usage(format!(
                "{place}: host table namespace '{}' must be letters, digits, '-' and '_'",
                namespace.escape_debug()
            ))
            .with_hint("write host settings as [x.<namespace>], such as [x.myapp]"));
        }
    }
    Ok(())
}

/// The tables as JSON. A TOML date or time becomes its RFC 3339 string,
/// since JSON has no such type.
pub fn to_json(tables: &HostTables) -> HostTablesJson {
    tables
        .iter()
        .map(|(namespace, table)| (namespace.clone(), table_to_json(table)))
        .collect()
}

fn table_to_json(table: &toml::Table) -> serde_json::Value {
    serde_json::Value::Object(
        table
            .iter()
            .map(|(key, value)| (key.clone(), value_to_json(value)))
            .collect(),
    )
}

fn value_to_json(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(text) => serde_json::Value::String(text.clone()),
        toml::Value::Integer(number) => serde_json::Value::from(*number),
        toml::Value::Float(number) => serde_json::Number::from_f64(*number)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(flag) => serde_json::Value::Bool(*flag),
        toml::Value::Datetime(datetime) => serde_json::Value::String(datetime.to_string()),
        toml::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        toml::Value::Table(table) => table_to_json(table),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Doc {
        x: HostTables,
    }

    #[test]
    fn host_tables_convert_to_json_with_dates_as_strings() {
        let doc: Doc = toml::from_str(
            r#"
[x.host]
default_level = "read"
retries = 3
ratio = 0.5
since = 2026-11-26
screens = [{ resource = "ui://changelog/preview", may_call = ["list_pull_requests"] }]
"#,
        )
        .unwrap();
        validate(&doc.x, "tuff-pack.toml").unwrap();
        let json = to_json(&doc.x);
        assert_eq!(
            json["host"],
            serde_json::json!({
                "default_level": "read",
                "retries": 3,
                "ratio": 0.5,
                "since": "2026-11-26",
                "screens": [{ "resource": "ui://changelog/preview", "may_call": ["list_pull_requests"] }]
            })
        );
    }

    #[test]
    fn a_namespace_must_be_a_plain_name_and_a_value_must_be_a_table() {
        let doc: Doc = toml::from_str("[x.\"my app\"]\nkey = 1\n").unwrap();
        let error = validate(&doc.x, "tuff-pack.toml").unwrap_err();
        assert!(error.to_string().contains("'my app'"), "{error}");
        assert!(toml::from_str::<Doc>("[x]\nhost = 1\n").is_err());
    }
}
