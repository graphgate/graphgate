use graphgate_schema::ComposedSchema;
use parser::parse_schema;

#[test]
fn test_datetime_scalar_conflict_should_succeed() {
    // Define two services with conflicting DateTime scalar definitions
    let service1_sdl = r#"
    type Query {
        currentTime: DateTime!
    }

    """
    Implement the DateTime<Utc> scalar
    The input/output is a string in RFC3339 format.
    """
    scalar DateTime
    "#;

    let service2_sdl = r#"
    type Query {
        scheduledTime: DateTime!
    }

    """
    A date-time string at UTC, such as 2019-12-03T09:54:33Z, compliant with the date-time format.
    """
    scalar DateTime
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should succeed with our fix
    let schema = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ])
    .expect("Failed to combine schemas with DateTime scalar conflict");

    // Verify that the DateTime scalar exists in the combined schema
    let datetime_type = schema.types.get("DateTime").expect("DateTime scalar not found in combined schema");
    
    // Verify that the first definition's description was kept
    assert_eq!(
        datetime_type.description.as_deref(),
        Some("Implement the DateTime<Utc> scalar\nThe input/output is a string in RFC3339 format.")
    );
}

#[test]
fn test_common_scalar_types_conflict_should_succeed() {
    // Test with other common scalar types
    let common_scalars = ["Date", "Time", "JSON", "UUID", "Email", "URL"];
    
    for scalar_name in common_scalars {
        let service1_sdl = format!(r#"
        type Query {{
            field1: {0}!
        }}

        """
        First definition of {0} scalar
        """
        scalar {0}
        "#, scalar_name);

        let service2_sdl = format!(r#"
        type Query {{
            field2: {0}!
        }}

        """
        Second definition of {0} scalar with different description
        """
        scalar {0}
        "#, scalar_name);

        // Parse the SDL into ServiceDocuments
        let service1_doc = parse_schema(&service1_sdl).expect("Failed to parse service1 SDL");
        let service2_doc = parse_schema(&service2_sdl).expect("Failed to parse service2 SDL");

        // Combine the services - this should succeed with our fix
        let schema = ComposedSchema::combine([
            ("service1".to_string(), service1_doc),
            ("service2".to_string(), service2_doc),
        ])
        .expect(&format!("Failed to combine schemas with {} scalar conflict", scalar_name));

        // Verify that the scalar exists in the combined schema
        let scalar_type = schema.types.get(scalar_name).expect(&format!("{} scalar not found in combined schema", scalar_name));
        
        // Verify that the first definition's description was kept
        assert_eq!(
            scalar_type.description.as_deref(),
            Some(&format!("First definition of {} scalar", scalar_name)[..])
        );
    }
} 