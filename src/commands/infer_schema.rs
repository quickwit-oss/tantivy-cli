use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::net::IpAddr;
use std::ops::AddAssign;
use std::path::Path;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InferredType {
    Text,
    Bool,
    U64,
    I64,
    F64,
    Date,
    IpAddr,
    Json,
}

#[derive(Debug)]
pub struct InferredField {
    pub name: String,
    pub field_type: InferredType,
}

#[derive(Debug)]
pub struct InferredSchema {
    pub fields: Vec<InferredField>,
    pub docs_analyzed: usize,
}

impl AddAssign for InferredType {
    fn add_assign(&mut self, rhs: InferredType) {
        use InferredType::*;
        *self = match (*self, rhs) {
            (lhs, rhs) if lhs == rhs => lhs,
            (Json, _) | (_, Json) => Json,

            // Numeric family
            (F64, _) | (_, F64) => F64,
            (U64, I64) | (I64, U64) => F64,

            // String-like family
            (Text, Date) | (Date, Text) => Text,
            (Text, IpAddr) | (IpAddr, Text) => Text,
            (Date, IpAddr) | (IpAddr, Date) => Text,

            // Most cross-family mixes are safest as Json.
            _ => Json,
        };
    }
}

fn inferred_type_from_value(value: &Value) -> Option<InferredType> {
    match value {
        Value::Null => None,
        Value::Bool(_) => Some(InferredType::Bool),
        Value::Number(number) => {
            if number.is_f64() {
                Some(InferredType::F64)
            } else if number.is_u64() {
                Some(InferredType::U64)
            } else if number.is_i64() {
                Some(InferredType::I64)
            } else {
                None
            }
        }
        Value::String(text) => {
            if OffsetDateTime::parse(text, &Rfc3339).is_ok() {
                Some(InferredType::Date)
            } else if text.parse::<IpAddr>().is_ok() {
                Some(InferredType::IpAddr)
            } else {
                Some(InferredType::Text)
            }
        }
        Value::Array(values) => {
            if values
                .iter()
                .any(|value| matches!(value, Value::Object(_) | Value::Array(_)))
            {
                return Some(InferredType::Json);
            }

            let mut observed_type: Option<InferredType> = None;
            for value in values {
                if let Some(seen_type) = inferred_type_from_value(value) {
                    match &mut observed_type {
                        Some(acc) => *acc += seen_type,
                        None => observed_type = Some(seen_type),
                    }
                }
            }
            observed_type
        }
        Value::Object(_) => Some(InferredType::Json),
    }
}

pub fn infer_schema_from_ndjson(path: &Path, sample_size: usize) -> Result<InferredSchema, String> {
    let file = File::open(path).map_err(|err| format!("failed to open {:?}: {:?}", path, err))?;
    let lines = BufReader::new(file)
        .lines()
        .map(|line| line.expect("failed to read line from ndjson"));
    Ok(infer_schema_from_lines(lines, sample_size))
}

fn infer_schema_from_lines<I>(lines: I, sample_size: usize) -> InferredSchema
where
    I: IntoIterator<Item = String>,
{
    let mut types_by_field = BTreeMap::<String, InferredType>::new();
    let mut docs_analyzed = 0usize;

    for line in lines {
        if sample_size != 0 && docs_analyzed >= sample_size {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let json_value: Value = serde_json::from_str(&line).expect("invalid json in ndjson input");
        let object = json_value
            .as_object()
            .expect("each ndjson line must be a root-level JSON object");
        docs_analyzed += 1;
        for (field_name, value) in object {
            if let Some(seen_type) = inferred_type_from_value(value) {
                match types_by_field.get_mut(field_name) {
                    Some(acc) => *acc += seen_type,
                    None => {
                        types_by_field.insert(field_name.clone(), seen_type);
                    }
                }
            }
        }
    }

    assert!(
        docs_analyzed > 0,
        "no root-level JSON object documents found"
    );

    let mut fields = Vec::new();
    for (name, inferred_type) in types_by_field {
        fields.push(InferredField {
            name,
            field_type: inferred_type,
        });
    }

    InferredSchema {
        fields,
        docs_analyzed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn infer(lines: &[&str]) -> InferredSchema {
        let lines = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        infer_schema_from_lines(lines, 0)
    }

    fn field_type(schema: &InferredSchema, name: &str) -> Option<InferredType> {
        schema
            .fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| field.field_type)
    }

    #[test]
    fn root_object_field_is_json() {
        let schema = infer(&[r#"{"custom":{"k":1}}"#]);
        assert_eq!(field_type(&schema, "custom"), Some(InferredType::Json));
    }

    #[test]
    fn numeric_mixture_promotes_to_f64() {
        let schema = infer(&[r#"{"a":1}"#, r#"{"a":1.5}"#]);
        assert_eq!(field_type(&schema, "a"), Some(InferredType::F64));
    }

    #[test]
    fn array_with_object_becomes_json() {
        let schema = infer(&[r#"{"arr":[{"x":1}]}"#]);
        assert_eq!(field_type(&schema, "arr"), Some(InferredType::Json));
    }

    #[test]
    fn mixed_string_kinds_fallback_to_text() {
        let schema = infer(&[
            r#"{"msg":"2025-10-24T17:45:38.379Z"}"#,
            r#"{"msg":"plain text"}"#,
        ]);
        assert_eq!(field_type(&schema, "msg"), Some(InferredType::Text));
    }

    #[test]
    fn number_plus_string_becomes_json() {
        let schema = infer(&[r#"{"x":1}"#, r#"{"x":"hello"}"#]);
        assert_eq!(field_type(&schema, "x"), Some(InferredType::Json));
    }

    #[test]
    #[should_panic(expected = "no root-level JSON object documents found")]
    fn empty_input_panics() {
        let _ = infer(&[]);
    }
}
