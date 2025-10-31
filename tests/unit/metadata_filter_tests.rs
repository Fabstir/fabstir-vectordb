// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

//! Unit tests for metadata filter parsing and evaluation
//!
//! Tests the MetadataFilter enum which provides a query language for filtering
//! search results based on metadata criteria.

use serde_json::json;
use vector_db::core::metadata_filter::{MetadataFilter, FilterError};

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
fn test_equals_filter_boolean() {
    let filter = MetadataFilter::Equals {
        field: "published".to_string(),
        value: json!(true),
    };

    let metadata = json!({
        "published": true,
        "title": "Article"
    });

    assert!(filter.matches(&metadata));

    let metadata_false = json!({
        "published": false,
        "title": "Draft"
    });

    assert!(!filter.matches(&metadata_false));
}

#[test]
fn test_in_filter() {
    let filter = MetadataFilter::In {
        field: "status".to_string(),
        values: vec![json!("active"), json!("pending"), json!("review")],
    };

    let metadata_active = json!({
        "status": "active",
        "id": "123"
    });

    assert!(filter.matches(&metadata_active));

    let metadata_pending = json!({
        "status": "pending",
        "id": "456"
    });

    assert!(filter.matches(&metadata_pending));

    let metadata_archived = json!({
        "status": "archived",
        "id": "789"
    });

    assert!(!filter.matches(&metadata_archived));
}

#[test]
fn test_in_filter_numbers() {
    let filter = MetadataFilter::In {
        field: "priority".to_string(),
        values: vec![json!(1), json!(2), json!(3)],
    };

    let metadata_1 = json!({"priority": 1});
    let metadata_2 = json!({"priority": 2});
    let metadata_5 = json!({"priority": 5});

    assert!(filter.matches(&metadata_1));
    assert!(filter.matches(&metadata_2));
    assert!(!filter.matches(&metadata_5));
}

#[test]
fn test_range_filter_both_bounds() {
    let filter = MetadataFilter::Range {
        field: "age".to_string(),
        min: Some(18.0),
        max: Some(65.0),
    };

    let metadata_25 = json!({"age": 25});
    let metadata_18 = json!({"age": 18});
    let metadata_65 = json!({"age": 65});
    let metadata_17 = json!({"age": 17});
    let metadata_66 = json!({"age": 66});

    assert!(filter.matches(&metadata_25));
    assert!(filter.matches(&metadata_18)); // Inclusive min
    assert!(filter.matches(&metadata_65)); // Inclusive max
    assert!(!filter.matches(&metadata_17));
    assert!(!filter.matches(&metadata_66));
}

#[test]
fn test_range_filter_min_only() {
    let filter = MetadataFilter::Range {
        field: "score".to_string(),
        min: Some(50.0),
        max: None,
    };

    let metadata_50 = json!({"score": 50});
    let metadata_100 = json!({"score": 100});
    let metadata_49 = json!({"score": 49});

    assert!(filter.matches(&metadata_50));
    assert!(filter.matches(&metadata_100));
    assert!(!filter.matches(&metadata_49));
}

#[test]
fn test_range_filter_max_only() {
    let filter = MetadataFilter::Range {
        field: "temperature".to_string(),
        min: None,
        max: Some(100.0),
    };

    let metadata_0 = json!({"temperature": 0});
    let metadata_100 = json!({"temperature": 100});
    let metadata_101 = json!({"temperature": 101});

    assert!(filter.matches(&metadata_0));
    assert!(filter.matches(&metadata_100));
    assert!(!filter.matches(&metadata_101));
}

#[test]
fn test_and_combinator_all_match() {
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
        "published": true,
        "title": "AI News"
    });

    assert!(filter.matches(&metadata_match));

    let metadata_no_match = json!({
        "category": "technology",
        "published": false,
        "title": "Draft"
    });

    assert!(!filter.matches(&metadata_no_match));
}

#[test]
fn test_and_combinator_empty() {
    let filter = MetadataFilter::And(vec![]);

    let metadata = json!({"any": "data"});

    // Empty AND should match everything (vacuous truth)
    assert!(filter.matches(&metadata));
}

#[test]
fn test_or_combinator_any_match() {
    let filter = MetadataFilter::Or(vec![
        MetadataFilter::Equals {
            field: "status".to_string(),
            value: json!("urgent"),
        },
        MetadataFilter::Range {
            field: "priority".to_string(),
            min: Some(8.0),
            max: None,
        },
    ]);

    let metadata_urgent = json!({
        "status": "urgent",
        "priority": 5
    });

    assert!(filter.matches(&metadata_urgent));

    let metadata_high_priority = json!({
        "status": "normal",
        "priority": 9
    });

    assert!(filter.matches(&metadata_high_priority));

    let metadata_no_match = json!({
        "status": "normal",
        "priority": 3
    });

    assert!(!filter.matches(&metadata_no_match));
}

#[test]
fn test_or_combinator_empty() {
    let filter = MetadataFilter::Or(vec![]);

    let metadata = json!({"any": "data"});

    // Empty OR should match nothing
    assert!(!filter.matches(&metadata));
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

    let metadata_no_match = json!({
        "user": {
            "id": "456",
            "name": "Bob"
        }
    });

    assert!(!filter.matches(&metadata_no_match));
}

#[test]
fn test_deeply_nested_field_access() {
    let filter = MetadataFilter::Equals {
        field: "data.location.city".to_string(),
        value: json!("London"),
    };

    let metadata = json!({
        "data": {
            "location": {
                "city": "London",
                "country": "UK"
            }
        }
    });

    assert!(filter.matches(&metadata));

    let metadata_no_match = json!({
        "data": {
            "location": {
                "city": "Paris",
                "country": "France"
            }
        }
    });

    assert!(!filter.matches(&metadata_no_match));
}

#[test]
fn test_array_field_matching_contains() {
    let filter = MetadataFilter::Equals {
        field: "tags".to_string(),
        value: json!("ai"),
    };

    let metadata_with_tag = json!({
        "tags": ["ai", "ml", "technology"],
        "title": "AI Article"
    });

    assert!(filter.matches(&metadata_with_tag));

    let metadata_without_tag = json!({
        "tags": ["sports", "news"],
        "title": "Sports Article"
    });

    assert!(!filter.matches(&metadata_without_tag));
}

#[test]
fn test_filter_parsing_equals_from_json() {
    let json_filter = json!({
        "category": "technology"
    });

    let filter = MetadataFilter::from_json(&json_filter).unwrap();

    let metadata = json!({"category": "technology"});
    assert!(filter.matches(&metadata));
}

#[test]
fn test_filter_parsing_in_from_json() {
    let json_filter = json!({
        "status": {
            "$in": ["active", "pending", "review"]
        }
    });

    let filter = MetadataFilter::from_json(&json_filter).unwrap();

    let metadata_active = json!({"status": "active"});
    let metadata_archived = json!({"status": "archived"});

    assert!(filter.matches(&metadata_active));
    assert!(!filter.matches(&metadata_archived));
}

#[test]
fn test_filter_parsing_range_from_json() {
    let json_filter = json!({
        "age": {
            "$gte": 18,
            "$lte": 65
        }
    });

    let filter = MetadataFilter::from_json(&json_filter).unwrap();

    let metadata_25 = json!({"age": 25});
    let metadata_17 = json!({"age": 17});

    assert!(filter.matches(&metadata_25));
    assert!(!filter.matches(&metadata_17));
}

#[test]
fn test_filter_parsing_and_from_json() {
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

    let metadata_no_match = json!({
        "category": "technology",
        "published": false
    });

    assert!(filter.matches(&metadata_match));
    assert!(!filter.matches(&metadata_no_match));
}

#[test]
fn test_filter_parsing_or_from_json() {
    let json_filter = json!({
        "$or": [
            {"status": "urgent"},
            {"priority": {"$gte": 8}}
        ]
    });

    let filter = MetadataFilter::from_json(&json_filter).unwrap();

    let metadata_urgent = json!({"status": "urgent", "priority": 5});
    let metadata_high_priority = json!({"status": "normal", "priority": 9});
    let metadata_no_match = json!({"status": "normal", "priority": 3});

    assert!(filter.matches(&metadata_urgent));
    assert!(filter.matches(&metadata_high_priority));
    assert!(!filter.matches(&metadata_no_match));
}

#[test]
fn test_filter_evaluation_against_metadata() {
    let filter = MetadataFilter::And(vec![
        MetadataFilter::Equals {
            field: "category".to_string(),
            value: json!("technology"),
        },
        MetadataFilter::Or(vec![
            MetadataFilter::Equals {
                field: "featured".to_string(),
                value: json!(true),
            },
            MetadataFilter::Range {
                field: "views".to_string(),
                min: Some(1000.0),
                max: None,
            },
        ]),
    ]);

    let metadata_featured = json!({
        "category": "technology",
        "featured": true,
        "views": 500
    });

    assert!(filter.matches(&metadata_featured));

    let metadata_high_views = json!({
        "category": "technology",
        "featured": false,
        "views": 5000
    });

    assert!(filter.matches(&metadata_high_views));

    let metadata_no_match = json!({
        "category": "sports",
        "featured": false,
        "views": 500
    });

    assert!(!filter.matches(&metadata_no_match));
}

#[test]
fn test_invalid_filter_syntax() {
    let json_filter = json!({
        "$invalid": "operator"
    });

    let result = MetadataFilter::from_json(&json_filter);
    assert!(result.is_err());

    if let Err(FilterError::UnsupportedOperator(op)) = result {
        assert_eq!(op, "$invalid");
    } else {
        panic!("Expected UnsupportedOperator error");
    }
}

#[test]
fn test_invalid_range_no_bounds() {
    // Range filter with neither min nor max is invalid
    let json_filter = json!({
        "age": {}
    });

    let result = MetadataFilter::from_json(&json_filter);
    assert!(result.is_err());

    if let Err(FilterError::InvalidSyntax(_)) = result {
        // Expected
    } else {
        panic!("Expected InvalidSyntax error");
    }
}

#[test]
fn test_missing_field_no_match() {
    let filter = MetadataFilter::Equals {
        field: "missing_field".to_string(),
        value: json!("value"),
    };

    let metadata = json!({
        "other_field": "data"
    });

    // Missing field should not match
    assert!(!filter.matches(&metadata));
}

#[test]
fn test_nested_missing_field_no_match() {
    let filter = MetadataFilter::Equals {
        field: "user.email".to_string(),
        value: json!("test@example.com"),
    };

    let metadata = json!({
        "user": {
            "id": "123"
            // email field missing
        }
    });

    assert!(!filter.matches(&metadata));
}

#[test]
fn test_complex_nested_filter() {
    let json_filter = json!({
        "$and": [
            {
                "article.category": "technology"
            },
            {
                "$or": [
                    {"article.views": {"$gte": 1000}},
                    {"article.featured": true}
                ]
            },
            {
                "author.verified": true
            }
        ]
    });

    let filter = MetadataFilter::from_json(&json_filter).unwrap();

    let metadata_match = json!({
        "article": {
            "category": "technology",
            "views": 5000,
            "featured": false
        },
        "author": {
            "verified": true,
            "name": "Alice"
        }
    });

    assert!(filter.matches(&metadata_match));

    let metadata_no_match = json!({
        "article": {
            "category": "technology",
            "views": 500,
            "featured": false
        },
        "author": {
            "verified": true
        }
    });

    assert!(!filter.matches(&metadata_no_match));
}
