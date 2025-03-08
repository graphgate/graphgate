use graphgate_schema::{CombineError, ComposedSchema};
use parser::parse_schema;

#[test]
fn test_fields_require_shareable_directive() {
    // Define two services with the same field but without @shareable
    let service1_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String!
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    let service2_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String!
        email: String!
    }

    type Query {
        users: [User!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should fail because 'name' is not marked as @shareable
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination failed with a FieldConflicted error
    match result {
        Err(CombineError::FieldConflicted { type_name, field_name }) => {
            assert_eq!(type_name, "User");
            assert_eq!(field_name, "name");
        },
        Ok(_) => panic!("Expected combination to fail due to non-shareable field"),
        Err(e) => panic!("Expected FieldConflicted error, got: {:?}", e),
    }
}

#[test]
fn test_shareable_directive_allows_field_sharing() {
    // Define two services with the same field, marked as @shareable
    let service1_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String! @shareable
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    let service2_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String! @shareable
        email: String!
    }

    type Query {
        users: [User!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should succeed because 'name' is marked as @shareable
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination succeeded
    match result {
        Ok(schema) => {
            // Verify that the User type exists and has the name field
            let user_type = schema
                .types
                .get("User")
                .expect("User type not found in combined schema");
            assert!(
                user_type.fields.contains_key("name"),
                "name field not found in User type"
            );
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}
