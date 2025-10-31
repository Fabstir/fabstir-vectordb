// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

//! Metadata filtering for search results
//!
//! Provides a MongoDB-style query language for filtering vectors based on metadata.
//! Supports equality, range, set membership, and boolean combinators.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

/// Errors that can occur during filter parsing or evaluation
#[derive(Error, Debug, Clone, PartialEq)]
pub enum FilterError {
    #[error("Invalid filter syntax: {0}")]
    InvalidSyntax(String),

    #[error("Unsupported operator: {0}")]
    UnsupportedOperator(String),

    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },
}

/// Metadata filter for querying vectors
///
/// Provides a query language similar to MongoDB for filtering search results.
/// Filters can be combined using AND/OR logic and support nested field access.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MetadataFilter {
    /// Exact match: `{ "field": "value" }`
    Equals {
        field: String,
        value: JsonValue,
    },

    /// Set membership: `{ "field": { "$in": ["val1", "val2"] } }`
    In {
        field: String,
        values: Vec<JsonValue>,
    },

    /// Range query: `{ "age": { "$gte": 18, "$lte": 65 } }` or `{ "score": { "$gt": 40, "$lt": 100 } }`
    Range {
        field: String,
        min: Option<f64>,
        max: Option<f64>,
        min_inclusive: bool, // true for $gte, false for $gt
        max_inclusive: bool, // true for $lte, false for $lt
    },

    /// All sub-filters must match: `{ "$and": [filter1, filter2] }`
    And(Vec<MetadataFilter>),

    /// At least one sub-filter must match: `{ "$or": [filter1, filter2] }`
    Or(Vec<MetadataFilter>),
}

impl MetadataFilter {
    /// Parse a filter from JSON
    ///
    /// # Examples
    ///
    /// ```
    /// use serde_json::json;
    /// use vector_db::core::metadata_filter::MetadataFilter;
    ///
    /// // Simple equality
    /// let filter = MetadataFilter::from_json(&json!({"category": "tech"})).unwrap();
    ///
    /// // Range query
    /// let filter = MetadataFilter::from_json(&json!({
    ///     "age": { "$gte": 18, "$lte": 65 }
    /// })).unwrap();
    ///
    /// // Complex combination
    /// let filter = MetadataFilter::from_json(&json!({
    ///     "$and": [
    ///         {"category": "tech"},
    ///         {"published": true}
    ///     ]
    /// })).unwrap();
    /// ```
    pub fn from_json(value: &JsonValue) -> Result<Self, FilterError> {
        match value {
            JsonValue::Object(map) => {
                // Check for special operators
                if let Some(and_filters) = map.get("$and") {
                    return Self::parse_and(and_filters);
                }

                if let Some(or_filters) = map.get("$or") {
                    return Self::parse_or(or_filters);
                }

                // Check for unsupported top-level operators
                for key in map.keys() {
                    if key.starts_with('$') && key != "$and" && key != "$or" {
                        return Err(FilterError::UnsupportedOperator(key.clone()));
                    }
                }

                // Single field filter
                if map.len() == 1 {
                    let (field, field_value) = map.iter().next().unwrap();
                    return Self::parse_field_filter(field, field_value);
                }

                // Multiple fields - implicit AND
                let mut filters = Vec::new();
                for (field, field_value) in map {
                    filters.push(Self::parse_field_filter(field, field_value)?);
                }
                Ok(MetadataFilter::And(filters))
            }
            _ => Err(FilterError::InvalidSyntax(
                "Filter must be a JSON object".to_string(),
            )),
        }
    }

    /// Parse an AND combinator
    fn parse_and(value: &JsonValue) -> Result<Self, FilterError> {
        match value {
            JsonValue::Array(filters) => {
                let mut parsed_filters = Vec::new();
                for filter in filters {
                    parsed_filters.push(Self::from_json(filter)?);
                }
                Ok(MetadataFilter::And(parsed_filters))
            }
            _ => Err(FilterError::InvalidSyntax(
                "$and must be an array".to_string(),
            )),
        }
    }

    /// Parse an OR combinator
    fn parse_or(value: &JsonValue) -> Result<Self, FilterError> {
        match value {
            JsonValue::Array(filters) => {
                let mut parsed_filters = Vec::new();
                for filter in filters {
                    parsed_filters.push(Self::from_json(filter)?);
                }
                Ok(MetadataFilter::Or(parsed_filters))
            }
            _ => Err(FilterError::InvalidSyntax(
                "$or must be an array".to_string(),
            )),
        }
    }

    /// Parse a single field filter
    fn parse_field_filter(field: &str, value: &JsonValue) -> Result<Self, FilterError> {
        match value {
            JsonValue::Object(ops) => {
                // Check for $in operator
                if let Some(in_values) = ops.get("$in") {
                    return Self::parse_in(field, in_values);
                }

                // Check for range operators ($gte, $gt, $lte, $lt)
                let min_gte = ops.get("$gte").and_then(|v| v.as_f64());
                let min_gt = ops.get("$gt").and_then(|v| v.as_f64());
                let max_lte = ops.get("$lte").and_then(|v| v.as_f64());
                let max_lt = ops.get("$lt").and_then(|v| v.as_f64());

                // Determine min bound and inclusivity
                let (min, min_inclusive) = match (min_gte, min_gt) {
                    (Some(gte), Some(gt)) => {
                        return Err(FilterError::InvalidSyntax(
                            "Cannot use both $gte and $gt in the same range filter".to_string(),
                        ));
                    }
                    (Some(gte), None) => (Some(gte), true),
                    (None, Some(gt)) => (Some(gt), false),
                    (None, None) => (None, true), // default to inclusive
                };

                // Determine max bound and inclusivity
                let (max, max_inclusive) = match (max_lte, max_lt) {
                    (Some(lte), Some(lt)) => {
                        return Err(FilterError::InvalidSyntax(
                            "Cannot use both $lte and $lt in the same range filter".to_string(),
                        ));
                    }
                    (Some(lte), None) => (Some(lte), true),
                    (None, Some(lt)) => (Some(lt), false),
                    (None, None) => (None, true), // default to inclusive
                };

                if min.is_some() || max.is_some() {
                    return Ok(MetadataFilter::Range {
                        field: field.to_string(),
                        min,
                        max,
                        min_inclusive,
                        max_inclusive,
                    });
                }

                // Check for unsupported operators
                for key in ops.keys() {
                    if key.starts_with('$')
                        && key != "$in"
                        && key != "$gte"
                        && key != "$gt"
                        && key != "$lte"
                        && key != "$lt"
                    {
                        return Err(FilterError::UnsupportedOperator(key.clone()));
                    }
                }

                // Empty object is invalid - must have at least one operator or value
                if ops.is_empty() {
                    return Err(FilterError::InvalidSyntax(
                        format!("Empty object for field '{}' - must specify a value or operator", field),
                    ));
                }

                // If no recognized operators, treat as equals with nested object
                Ok(MetadataFilter::Equals {
                    field: field.to_string(),
                    value: value.clone(),
                })
            }
            _ => {
                // Simple value - equals filter
                Ok(MetadataFilter::Equals {
                    field: field.to_string(),
                    value: value.clone(),
                })
            }
        }
    }

    /// Parse an $in operator
    fn parse_in(field: &str, value: &JsonValue) -> Result<Self, FilterError> {
        match value {
            JsonValue::Array(values) => Ok(MetadataFilter::In {
                field: field.to_string(),
                values: values.clone(),
            }),
            _ => Err(FilterError::InvalidSyntax(
                "$in value must be an array".to_string(),
            )),
        }
    }

    /// Check if metadata matches this filter
    ///
    /// # Examples
    ///
    /// ```
    /// use serde_json::json;
    /// use vector_db::core::metadata_filter::MetadataFilter;
    ///
    /// let filter = MetadataFilter::Equals {
    ///     field: "category".to_string(),
    ///     value: json!("tech"),
    /// };
    ///
    /// let metadata = json!({"category": "tech", "published": true});
    /// assert!(filter.matches(&metadata));
    /// ```
    pub fn matches(&self, metadata: &JsonValue) -> bool {
        match self {
            MetadataFilter::Equals { field, value } => {
                if let Some(field_value) = get_field(metadata, field) {
                    // Special handling for array fields - check if value is in array
                    if let JsonValue::Array(arr) = field_value {
                        arr.contains(value)
                    } else {
                        field_value == value
                    }
                } else {
                    false
                }
            }

            MetadataFilter::In { field, values } => {
                if let Some(field_value) = get_field(metadata, field) {
                    values.contains(field_value)
                } else {
                    false
                }
            }

            MetadataFilter::Range { field, min, max, min_inclusive, max_inclusive } => {
                if let Some(field_value) = get_field(metadata, field) {
                    if let Some(num) = field_value.as_f64() {
                        let min_ok = min.map_or(true, |m| {
                            if *min_inclusive {
                                num >= m  // $gte
                            } else {
                                num > m   // $gt
                            }
                        });
                        let max_ok = max.map_or(true, |m| {
                            if *max_inclusive {
                                num <= m  // $lte
                            } else {
                                num < m   // $lt
                            }
                        });
                        min_ok && max_ok
                    } else {
                        false
                    }
                } else {
                    false
                }
            }

            MetadataFilter::And(filters) => {
                // Empty AND matches everything (vacuous truth)
                if filters.is_empty() {
                    return true;
                }
                filters.iter().all(|f| f.matches(metadata))
            }

            MetadataFilter::Or(filters) => {
                // Empty OR matches nothing
                if filters.is_empty() {
                    return false;
                }
                filters.iter().any(|f| f.matches(metadata))
            }
        }
    }
}

/// Get a field value from metadata using dot notation
///
/// Supports nested field access: "user.id" → metadata["user"]["id"]
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use vector_db::core::metadata_filter::get_field;
///
/// let metadata = json!({
///     "user": {
///         "id": "123",
///         "name": "Alice"
///     }
/// });
///
/// assert_eq!(get_field(&metadata, "user.id"), Some(&json!("123")));
/// assert_eq!(get_field(&metadata, "user.name"), Some(&json!("Alice")));
/// assert_eq!(get_field(&metadata, "user.email"), None);
/// ```
pub fn get_field<'a>(metadata: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = metadata;

    for part in parts {
        match current {
            JsonValue::Object(map) => {
                current = map.get(part)?;
            }
            _ => return None,
        }
    }

    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_equals_filter_string() {
        let filter = MetadataFilter::Equals {
            field: "category".to_string(),
            value: json!("technology"),
        };

        let metadata = json!({
            "category": "technology",
            "title": "AI News"
        });

        assert!(filter.matches(&metadata));

        let metadata_no_match = json!({
            "category": "sports",
            "title": "Game Results"
        });

        assert!(!filter.matches(&metadata_no_match));
    }

    #[test]
    fn test_equals_filter_number() {
        let filter = MetadataFilter::Equals {
            field: "priority".to_string(),
            value: json!(1),
        };

        let metadata = json!({
            "priority": 1,
            "status": "active"
        });

        assert!(filter.matches(&metadata));

        let metadata_no_match = json!({
            "priority": 2,
            "status": "active"
        });

        assert!(!filter.matches(&metadata_no_match));
    }

    #[test]
    fn test_in_filter() {
        let filter = MetadataFilter::In {
            field: "status".to_string(),
            values: vec![json!("active"), json!("pending")],
        };

        let metadata_active = json!({"status": "active"});
        assert!(filter.matches(&metadata_active));

        let metadata_archived = json!({"status": "archived"});
        assert!(!filter.matches(&metadata_archived));
    }

    #[test]
    fn test_range_filter() {
        let filter = MetadataFilter::Range {
            field: "age".to_string(),
            min: Some(18.0),
            max: Some(65.0),
            min_inclusive: true,
            max_inclusive: true,
        };

        let metadata_25 = json!({"age": 25});
        assert!(filter.matches(&metadata_25));

        let metadata_17 = json!({"age": 17});
        assert!(!filter.matches(&metadata_17));
    }

    #[test]
    fn test_and_combinator() {
        let filter = MetadataFilter::And(vec![
            MetadataFilter::Equals {
                field: "category".to_string(),
                value: json!("technology"),
            },
            MetadataFilter::Equals {
                field: "published".to_string(),
                value: json!(true),
            },
        ]);

        let metadata_match = json!({
            "category": "technology",
            "published": true
        });

        assert!(filter.matches(&metadata_match));

        let metadata_no_match = json!({
            "category": "technology",
            "published": false
        });

        assert!(!filter.matches(&metadata_no_match));
    }

    #[test]
    fn test_or_combinator() {
        let filter = MetadataFilter::Or(vec![
            MetadataFilter::Equals {
                field: "status".to_string(),
                value: json!("urgent"),
            },
            MetadataFilter::Range {
                field: "priority".to_string(),
                min: Some(8.0),
                max: None,
                min_inclusive: true,
                max_inclusive: true,
            },
        ]);

        let metadata_urgent = json!({"status": "urgent", "priority": 5});
        assert!(filter.matches(&metadata_urgent));

        let metadata_no_match = json!({"status": "normal", "priority": 3});
        assert!(!filter.matches(&metadata_no_match));
    }

    #[test]
    fn test_nested_field_access() {
        let filter = MetadataFilter::Equals {
            field: "user.id".to_string(),
            value: json!("123"),
        };

        let metadata = json!({
            "user": {
                "id": "123",
                "name": "Alice"
            }
        });

        assert!(filter.matches(&metadata));
    }

    #[test]
    fn test_array_field_matching() {
        let filter = MetadataFilter::Equals {
            field: "tags".to_string(),
            value: json!("ai"),
        };

        let metadata = json!({
            "tags": ["ai", "ml", "technology"]
        });

        assert!(filter.matches(&metadata));
    }

    #[test]
    fn test_from_json_equals() {
        let json_filter = json!({"category": "technology"});
        let filter = MetadataFilter::from_json(&json_filter).unwrap();

        let metadata = json!({"category": "technology"});
        assert!(filter.matches(&metadata));
    }

    #[test]
    fn test_from_json_in() {
        let json_filter = json!({
            "status": {"$in": ["active", "pending"]}
        });

        let filter = MetadataFilter::from_json(&json_filter).unwrap();

        let metadata = json!({"status": "active"});
        assert!(filter.matches(&metadata));
    }

    #[test]
    fn test_from_json_range() {
        let json_filter = json!({
            "age": {"$gte": 18, "$lte": 65}
        });

        let filter = MetadataFilter::from_json(&json_filter).unwrap();

        let metadata_25 = json!({"age": 25});
        assert!(filter.matches(&metadata_25));
    }

    #[test]
    fn test_from_json_and() {
        let json_filter = json!({
            "$and": [
                {"category": "technology"},
                {"published": true}
            ]
        });

        let filter = MetadataFilter::from_json(&json_filter).unwrap();

        let metadata_match = json!({
            "category": "technology",
            "published": true
        });

        assert!(filter.matches(&metadata_match));
    }

    #[test]
    fn test_invalid_operator() {
        let json_filter = json!({"$invalid": "test"});
        let result = MetadataFilter::from_json(&json_filter);

        assert!(result.is_err());
        assert!(matches!(result, Err(FilterError::UnsupportedOperator(_))));
    }

    #[test]
    fn test_get_field() {
        let metadata = json!({
            "user": {
                "id": "123",
                "profile": {
                    "email": "test@example.com"
                }
            }
        });

        assert_eq!(get_field(&metadata, "user.id"), Some(&json!("123")));
        assert_eq!(
            get_field(&metadata, "user.profile.email"),
            Some(&json!("test@example.com"))
        );
        assert_eq!(get_field(&metadata, "user.missing"), None);
    }
}
