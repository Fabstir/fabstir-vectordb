// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

//! Metadata schema definition and validation
//!
//! Provides optional schema validation for vector metadata to ensure data consistency.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Error types for schema validation
#[derive(Error, Debug, Clone, PartialEq)]
pub enum SchemaError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid type for field '{field}': expected {expected}, found {found}")]
    InvalidType {
        field: String,
        expected: String,
        found: String,
    },

    #[error("Invalid array element at index {index} in field '{field}': expected {expected}, found {found}")]
    InvalidArrayElement {
        field: String,
        index: usize,
        expected: String,
        found: String,
    },
}

/// Field type definition for metadata schema
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldType {
    /// String type
    String,

    /// Number type (integer or float)
    Number,

    /// Boolean type
    Boolean,

    /// Array type with element type
    Array(Box<FieldType>),

    /// Object type with nested fields
    Object(HashMap<String, FieldType>),
}

impl FieldType {
    /// Get type name as string for error messages
    pub fn type_name(&self) -> String {
        match self {
            FieldType::String => "String".to_string(),
            FieldType::Number => "Number".to_string(),
            FieldType::Boolean => "Boolean".to_string(),
            FieldType::Array(inner) => format!("Array<{}>", inner.type_name()),
            FieldType::Object(_) => "Object".to_string(),
        }
    }

    /// Validate a value against this field type
    pub fn validate_value(&self, field_name: &str, value: &Value) -> Result<(), SchemaError> {
        // Null values are allowed for all types (optional fields)
        if value.is_null() {
            return Ok(());
        }

        match self {
            FieldType::String => {
                if !value.is_string() {
                    return Err(SchemaError::InvalidType {
                        field: field_name.to_string(),
                        expected: self.type_name(),
                        found: get_value_type_name(value),
                    });
                }
            }
            FieldType::Number => {
                if !value.is_number() {
                    return Err(SchemaError::InvalidType {
                        field: field_name.to_string(),
                        expected: self.type_name(),
                        found: get_value_type_name(value),
                    });
                }
            }
            FieldType::Boolean => {
                if !value.is_boolean() {
                    return Err(SchemaError::InvalidType {
                        field: field_name.to_string(),
                        expected: self.type_name(),
                        found: get_value_type_name(value),
                    });
                }
            }
            FieldType::Array(element_type) => {
                if let Some(array) = value.as_array() {
                    for (index, element) in array.iter().enumerate() {
                        if !element.is_null() {
                            element_type
                                .validate_value(&format!("{}[{}]", field_name, index), element)
                                .map_err(|e| match e {
                                    SchemaError::InvalidType { expected, found, .. } => {
                                        SchemaError::InvalidArrayElement {
                                            field: field_name.to_string(),
                                            index,
                                            expected,
                                            found,
                                        }
                                    }
                                    _ => e,
                                })?;
                        }
                    }
                } else {
                    return Err(SchemaError::InvalidType {
                        field: field_name.to_string(),
                        expected: self.type_name(),
                        found: get_value_type_name(value),
                    });
                }
            }
            FieldType::Object(fields) => {
                if let Some(obj) = value.as_object() {
                    for (key, field_type) in fields {
                        if let Some(field_value) = obj.get(key) {
                            field_type.validate_value(
                                &format!("{}.{}", field_name, key),
                                field_value,
                            )?;
                        }
                    }
                } else {
                    return Err(SchemaError::InvalidType {
                        field: field_name.to_string(),
                        expected: self.type_name(),
                        found: get_value_type_name(value),
                    });
                }
            }
        }

        Ok(())
    }
}

/// Metadata schema definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataSchema {
    /// Field definitions (field_name -> field_type)
    pub fields: HashMap<String, FieldType>,

    /// Required field names
    pub required: HashSet<String>,
}

impl MetadataSchema {
    /// Create a new empty schema
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
            required: HashSet::new(),
        }
    }

    /// Add a field to the schema
    pub fn add_field(&mut self, name: impl Into<String>, field_type: FieldType, required: bool) {
        let name = name.into();
        self.fields.insert(name.clone(), field_type);
        if required {
            self.required.insert(name);
        }
    }

    /// Validate metadata against this schema
    pub fn validate(&self, metadata: &Value) -> Result<(), SchemaError> {
        if !metadata.is_object() {
            return Err(SchemaError::InvalidType {
                field: "metadata".to_string(),
                expected: "Object".to_string(),
                found: get_value_type_name(metadata),
            });
        }

        let obj = metadata.as_object().unwrap();

        // Check required fields
        for required_field in &self.required {
            if !obj.contains_key(required_field) {
                return Err(SchemaError::MissingField(required_field.clone()));
            }
        }

        // Validate field types
        for (field_name, field_type) in &self.fields {
            if let Some(value) = obj.get(field_name) {
                field_type.validate_value(field_name, value)?;
            }
        }

        Ok(())
    }
}

impl Default for MetadataSchema {
    fn default() -> Self {
        Self::new()
    }
}

/// Get type name of a JSON value for error messages
fn get_value_type_name(value: &Value) -> String {
    match value {
        Value::Null => "Null".to_string(),
        Value::Bool(_) => "Boolean".to_string(),
        Value::Number(_) => "Number".to_string(),
        Value::String(_) => "String".to_string(),
        Value::Array(_) => "Array".to_string(),
        Value::Object(_) => "Object".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_field_type_name() {
        assert_eq!(FieldType::String.type_name(), "String");
        assert_eq!(FieldType::Number.type_name(), "Number");
        assert_eq!(FieldType::Boolean.type_name(), "Boolean");
        assert_eq!(
            FieldType::Array(Box::new(FieldType::String)).type_name(),
            "Array<String>"
        );
        assert_eq!(
            FieldType::Object(HashMap::new()).type_name(),
            "Object"
        );
    }

    #[test]
    fn test_validate_string() {
        let field_type = FieldType::String;
        assert!(field_type.validate_value("test", &json!("hello")).is_ok());
        assert!(field_type.validate_value("test", &json!(123)).is_err());
        assert!(field_type.validate_value("test", &json!(null)).is_ok()); // Null allowed
    }

    #[test]
    fn test_validate_number() {
        let field_type = FieldType::Number;
        assert!(field_type.validate_value("test", &json!(123)).is_ok());
        assert!(field_type.validate_value("test", &json!(123.45)).is_ok());
        assert!(field_type.validate_value("test", &json!("123")).is_err());
    }

    #[test]
    fn test_validate_boolean() {
        let field_type = FieldType::Boolean;
        assert!(field_type.validate_value("test", &json!(true)).is_ok());
        assert!(field_type.validate_value("test", &json!(false)).is_ok());
        assert!(field_type.validate_value("test", &json!("true")).is_err());
    }

    #[test]
    fn test_validate_array() {
        let field_type = FieldType::Array(Box::new(FieldType::String));
        assert!(field_type
            .validate_value("test", &json!(["a", "b", "c"]))
            .is_ok());
        assert!(field_type
            .validate_value("test", &json!(["a", 123, "c"]))
            .is_err());
    }

    #[test]
    fn test_schema_builder() {
        let mut schema = MetadataSchema::new();
        schema.add_field("title", FieldType::String, true);
        schema.add_field("views", FieldType::Number, false);

        assert_eq!(schema.fields.len(), 2);
        assert_eq!(schema.required.len(), 1);
        assert!(schema.required.contains("title"));
    }
}
