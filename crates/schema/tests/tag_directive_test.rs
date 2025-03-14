use graphgate_schema::ComposedSchema;
use parser::parse_schema;
use tracing::debug;

#[test]
fn test_tag_directive_on_types() {
    // Define a service with @tag directives on types
    let service_sdl = r#"
    type Query {
        users: [User!]!
    }

    """
    User type with tags
    """
    type User @tag(name: "authenticated") @tag(name: "entity") {
        id: ID!
        name: String!
    }

    input UserInput @tag(name: "input") {
        name: String!
    }

    enum Role @tag(name: "enum") {
        ADMIN
        USER
    }

    interface Node @tag(name: "interface") {
        id: ID!
    }

    union SearchResult @tag(name: "union") = User
    "#;

    // Parse the SDL into a ServiceDocument
    let service_doc = parse_schema(service_sdl).expect("Failed to parse service SDL");

    // Combine the service
    let schema = ComposedSchema::combine([("service1".to_string(), service_doc)])
        .expect("Failed to combine schema with @tag directives");

    // Verify that the User type has the correct tags
    let user_type = schema
        .types
        .get("User")
        .expect("User type not found in combined schema");
    assert_eq!(user_type.tags.len(), 2);
    assert!(user_type.tags.contains(&"authenticated".to_string()));
    assert!(user_type.tags.contains(&"entity".to_string()));

    // Verify that the UserInput type has the correct tag
    let user_input_type = schema
        .types
        .get("UserInput")
        .expect("UserInput type not found in combined schema");
    assert_eq!(user_input_type.tags.len(), 1);
    assert!(user_input_type.tags.contains(&"input".to_string()));

    // Verify that the Role enum has the correct tag
    let role_enum = schema
        .types
        .get("Role")
        .expect("Role enum not found in combined schema");
    assert_eq!(role_enum.tags.len(), 1);
    assert!(role_enum.tags.contains(&"enum".to_string()));

    // Verify that the Node interface has the correct tag
    let node_interface = schema
        .types
        .get("Node")
        .expect("Node interface not found in combined schema");
    assert_eq!(node_interface.tags.len(), 1);
    assert!(node_interface.tags.contains(&"interface".to_string()));

    // Verify that the SearchResult union has the correct tag
    let search_result_union = schema
        .types
        .get("SearchResult")
        .expect("SearchResult union not found in combined schema");
    assert_eq!(search_result_union.tags.len(), 1);
    assert!(search_result_union.tags.contains(&"union".to_string()));
}

#[test]
fn test_tag_directive_on_enum_values() {
    // Define a service with @tag directives on enum values
    let service_sdl = r#"
    type Query {
        role: Role
    }

    enum Role {
        ADMIN @tag(name: "admin")
        USER @tag(name: "user") @tag(name: "default")
    }
    "#;

    // Parse the SDL into a ServiceDocument
    let service_doc = parse_schema(service_sdl).expect("Failed to parse service SDL");

    // Combine the service
    let schema = ComposedSchema::combine([("service1".to_string(), service_doc)])
        .expect("Failed to combine schema with @tag directives on enum values");

    // Verify that the Role.ADMIN enum value has the correct tag
    let role_enum = schema
        .types
        .get("Role")
        .expect("Role enum not found in combined schema");
    let admin_value = role_enum
        .enum_values
        .get("ADMIN")
        .expect("ADMIN value not found in Role enum");
    assert_eq!(admin_value.tags.len(), 1);
    assert!(admin_value.tags.contains(&"admin".to_string()));

    // Verify that the Role.USER enum value has the correct tags
    let user_value = role_enum
        .enum_values
        .get("USER")
        .expect("USER value not found in Role enum");
    assert_eq!(user_value.tags.len(), 2);
    assert!(user_value.tags.contains(&"user".to_string()));
    assert!(user_value.tags.contains(&"default".to_string()));
}

#[test]
fn test_tag_directive_on_input_fields() {
    // Define a service with @tag directives on input fields
    let service_sdl = r#"
    type Query {
        createUser(input: UserInput!): User!
    }

    type User {
        id: ID!
        name: String!
    }

    input UserInput {
        name: String! @tag(name: "required")
        email: String @tag(name: "optional") @tag(name: "contact")
    }
    "#;

    // Parse the SDL into a ServiceDocument
    let service_doc = parse_schema(service_sdl).expect("Failed to parse service SDL");

    // Combine the service
    let schema = ComposedSchema::combine([("service1".to_string(), service_doc)])
        .expect("Failed to combine schema with @tag directives on input fields");

    // Verify that the UserInput.name input field has the correct tag
    let user_input_type = schema
        .types
        .get("UserInput")
        .expect("UserInput type not found in combined schema");
    let name_field = user_input_type
        .input_fields
        .get("name")
        .expect("name field not found in UserInput type");
    assert_eq!(name_field.tags.len(), 1);
    assert!(name_field.tags.contains(&"required".to_string()));

    // Verify that the UserInput.email input field has the correct tags
    let email_field = user_input_type
        .input_fields
        .get("email")
        .expect("email field not found in UserInput type");
    assert_eq!(email_field.tags.len(), 2);
    assert!(email_field.tags.contains(&"optional".to_string()));
    assert!(email_field.tags.contains(&"contact".to_string()));
}

#[test]
fn test_tag_directive_federation_v2_compatibility() {
    // Define two services with @tag directives in a Federation v2 setup
    let service1_sdl = r#"
    extend schema @link(url: "https://specs.apollo.dev/federation/v2.0", import: ["@key", "@tag"])

    type Query {
        users: [User!]!
    }

    type User @key(fields: "id") @tag(name: "service1") {
        id: ID!
        name: String!
    }
    "#;

    let service2_sdl = r#"
    extend schema @link(url: "https://specs.apollo.dev/federation/v2.0", import: ["@key", "@tag"])

    type Query {
        user(id: ID!): User
    }

    type User @key(fields: "id") @tag(name: "service2") {
        id: ID!
        email: String
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services
    let schema = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ])
    .expect("Failed to combine schemas with @tag directives in Federation v2");

    // Verify that the User type has the correct tags
    let user_type = schema
        .types
        .get("User")
        .expect("User type not found in combined schema");

    // Print the actual tags for debugging
    debug!("User type tags: {:?}", user_type.tags);

    // Check that the User type has at least one tag
    assert!(!user_type.tags.is_empty());
    assert!(user_type.tags.contains(&"service1".to_string()) || user_type.tags.contains(&"service2".to_string()));
}
