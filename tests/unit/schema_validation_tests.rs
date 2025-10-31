// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use serde_json::json;
use std::collections::{HashMap, HashSet};
use vector_db::core::schema::*;

#[cfg(test)]
mod schema_definition_tests {
    use super::*;

    #[test]
    fn test_field_type_string() {
        let field_type = FieldType::String;
        assert!(matches!(field_type, FieldType::String));
    }

    #[test]
    fn test_field_type_number() {
        let field_type = FieldType::Number;
        assert!(matches!(field_type, FieldType::Number));
    }

    #[test]
    fn test_field_type_boolean() {
        let field_type = FieldType::Boolean;
        assert!(matches!(field_type, FieldType::Boolean));
    }

    #[test]
    fn test_field_type_array() {
        let field_type = FieldType::Array(Box::new(FieldType::String));
        if let FieldType::Array(inner) = field_type {
            assert!(matches!(*inner, FieldType::String));
        } else {
            panic!("Expected Array type");
        }
    }

    #[test]
    fn test_field_type_object() {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), FieldType::String);
        fields.insert("age".to_string(), FieldType::Number);

        let field_type = FieldType::Object(fields);
        if let FieldType::Object(obj_fields) = field_type {
            assert_eq!(obj_fields.len(), 2);
            assert!(matches!(obj_fields.get("name"), Some(FieldType::String)));
            assert!(matches!(obj_fields.get("age"), Some(FieldType::Number)));
        } else {
            panic!("Expected Object type");
        }
    }

    #[test]
    fn test_metadata_schema_creation() {
        let mut fields = HashMap::new();
        fields.insert("title".to_string(), FieldType::String);
        fields.insert("views".to_string(), FieldType::Number);
        fields.insert("published".to_string(), FieldType::Boolean);

        let mut required = HashSet::new();
        required.insert("title".to_string());

        let schema = MetadataSchema {
            fields,
            required,
        };

        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.required.len(), 1);
        assert!(schema.required.contains("title"));
    }
}

#[cfg(test)]
mod schema_validation_tests {
    use super::*;

    fn create_test_schema() -> MetadataSchema {
        let mut fields = HashMap::new();
        fields.insert("title".to_string(), FieldType::String);
        fields.insert("views".to_string(), FieldType::Number);
        fields.insert("published".to_string(), FieldType::Boolean);
        fields.insert("tags".to_string(), FieldType::Array(Box::new(FieldType::String)));

        let mut required = HashSet::new();
        required.insert("title".to_string());
        required.insert("views".to_string());

        MetadataSchema {
            fields,
            required,
        }
    }

    #[test]
    fn test_valid_metadata_passes() {
        let schema = create_test_schema();
        let metadata = json!({
            "title": "Test Document",
            "views": 100,
            "published": true,
            "tags": ["tech", "rust"]
        });

        let result = schema.validate(&metadata);
        assert!(result.is_ok(), "Valid metadata should pass validation");
    }

    #[test]
    fn test_valid_metadata_without_optional_fields() {
        let schema = create_test_schema();
        let metadata = json!({
            "title": "Test Document",
            "views": 100
        });

        let result = schema.validate(&metadata);
        assert!(result.is_ok(), "Metadata without optional fields should pass");
    }

    #[test]
    fn test_missing_required_field_rejected() {
        let schema = create_test_schema();
        let metadata = json!({
            "views": 100,
            "published": true
        });

        let result = schema.validate(&metadata);
        assert!(result.is_err(), "Missing required field should fail");

        if let Err(SchemaError::MissingField(field)) = result {
            assert_eq!(field, "title");
        } else {
            panic!("Expected MissingField error");
        }
    }

    #[test]
    fn test_wrong_type_string_rejected() {
        let schema = create_test_schema();
        let metadata = json!({
            "title": 123,  // Should be string
            "views": 100
        });

        let result = schema.validate(&metadata);
        assert!(result.is_err(), "Wrong type should fail");

        if let Err(SchemaError::InvalidType { field, expected, found }) = result {
            assert_eq!(field, "title");
            assert_eq!(expected, "String");
        } else {
            panic!("Expected InvalidType error");
        }
    }

    #[test]
    fn test_wrong_type_number_rejected() {
        let schema = create_test_schema();
        let metadata = json!({
            "title": "Test",
            "views": "100"  // Should be number
        });

        let result = schema.validate(&metadata);
        assert!(result.is_err(), "Wrong type should fail");

        if let Err(SchemaError::InvalidType { field, expected, .. }) = result {
            assert_eq!(field, "views");
            assert_eq!(expected, "Number");
        } else {
            panic!("Expected InvalidType error");
        }
    }

    #[test]
    fn test_wrong_type_boolean_rejected() {
        let schema = create_test_schema();
        let metadata = json!({
            "title": "Test",
            "views": 100,
            "published": "true"  // Should be boolean
        });

        let result = schema.validate(&metadata);
        assert!(result.is_err(), "Wrong type should fail");

        if let Err(SchemaError::InvalidType { field, expected, .. }) = result {
            assert_eq!(field, "published");
            assert_eq!(expected, "Boolean");
        } else {
            panic!("Expected InvalidType error");
        }
    }

    #[test]
    fn test_array_validation() {
        let schema = create_test_schema();
        let metadata = json!({
            "title": "Test",
            "views": 100,
            "tags": ["valid", "tags"]
        });

        let result = schema.validate(&metadata);
        assert!(result.is_ok(), "Valid array should pass");
    }

    #[test]
    fn test_array_wrong_element_type() {
        let schema = create_test_schema();
        let metadata = json!({
            "title": "Test",
            "views": 100,
            "tags": ["valid", 123]  // Second element should be string
        });

        let result = schema.validate(&metadata);
        assert!(result.is_err(), "Array with wrong element type should fail");
    }

    #[test]
    fn test_nested_object_validation() {
        let mut author_fields = HashMap::new();
        author_fields.insert("name".to_string(), FieldType::String);
        author_fields.insert("verified".to_string(), FieldType::Boolean);

        let mut fields = HashMap::new();
        fields.insert("title".to_string(), FieldType::String);
        fields.insert("author".to_string(), FieldType::Object(author_fields));

        let mut required = HashSet::new();
        required.insert("title".to_string());

        let schema = MetadataSchema {
            fields,
            required,
        };

        let metadata = json!({
            "title": "Test",
            "author": {
                "name": "Alice",
                "verified": true
            }
        });

        let result = schema.validate(&metadata);
        assert!(result.is_ok(), "Valid nested object should pass");
    }

    #[test]
    fn test_nested_object_invalid_field() {
        let mut author_fields = HashMap::new();
        author_fields.insert("name".to_string(), FieldType::String);
        author_fields.insert("verified".to_string(), FieldType::Boolean);

        let mut fields = HashMap::new();
        fields.insert("title".to_string(), FieldType::String);
        fields.insert("author".to_string(), FieldType::Object(author_fields));

        let schema = MetadataSchema {
            fields: fields,
            required: HashSet::new(),
        };

        let metadata = json!({
            "title": "Test",
            "author": {
                "name": "Alice",
                "verified": "yes"  // Should be boolean
            }
        });

        let result = schema.validate(&metadata);
        assert!(result.is_err(), "Nested object with invalid field should fail");
    }

    #[test]
    fn test_extra_fields_allowed() {
        let schema = create_test_schema();
        let metadata = json!({
            "title": "Test",
            "views": 100,
            "extra_field": "allowed"  // Not in schema
        });

        let result = schema.validate(&metadata);
        assert!(result.is_ok(), "Extra fields should be allowed");
    }

    #[test]
    fn test_null_value_for_optional_field() {
        let schema = create_test_schema();
        let metadata = json!({
            "title": "Test",
            "views": 100,
            "published": null
        });

        let result = schema.validate(&metadata);
        assert!(result.is_ok(), "Null value for optional field should be allowed");
    }
}
