use graphgate_schema::{CombineError, ComposedSchema, TypeKind};
use parser::parse_schema;

#[test]
fn test_fields_require_shareable_directive_when_incompatible_types() {
    // Define two services with the same field but with incompatible types
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
        name: Int!  # Different type (Int instead of String)
        email: String!
    }

    type Query {
        users: [User!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should fail because 'name' has incompatible types and is not marked as @shareable
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
        Ok(_) => panic!("Expected combination to fail due to incompatible field types"),
        Err(e) => panic!("Expected FieldConflicted error, got: {:?}", e),
    }
}

#[test]
fn test_fields_require_shareable_directive_for_incompatible_return_types() {
    // Define two services with the same field but with incompatible return types
    let service1_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        friends: [User!]!
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    let service2_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        friends: [String!]!  # Different return type
        email: String!
    }

    type Query {
        users: [User!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should fail because 'friends' has incompatible return types
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination failed with a FieldConflicted error
    match result {
        Err(CombineError::FieldConflicted { type_name, field_name }) => {
            assert_eq!(type_name, "User");
            assert_eq!(field_name, "friends");
        },
        Ok(_) => panic!("Expected combination to fail due to incompatible return types"),
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

#[test]
fn test_compatible_fields_require_shareable_directive() {
    // Define two services with the same field with compatible definitions
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
        name: String!  # Same type as in service1, but not marked as @shareable
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
fn test_compatible_fields_with_arguments_require_shareable() {
    // Define two services with the same field with compatible arguments
    let service1_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        posts(limit: Int = 10): [String!]!
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    let service2_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        posts(limit: Int = 10): [String!]!  # Same arguments as in service1, but not marked as @shareable
        email: String!
    }

    type Query {
        users: [User!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should fail because 'posts' is not marked as @shareable
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination failed with a FieldConflicted error
    match result {
        Err(CombineError::FieldConflicted { type_name, field_name }) => {
            assert_eq!(type_name, "User");
            assert_eq!(field_name, "posts");
        },
        Ok(_) => panic!("Expected combination to fail due to non-shareable field"),
        Err(e) => panic!("Expected FieldConflicted error, got: {:?}", e),
    }
}

#[test]
fn test_compatible_fields_with_different_descriptions() {
    // Define two services with the same field but different descriptions
    let service1_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        "Description from service1"
        name: String! @shareable
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    let service2_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        "Description from service2"
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
            let name_field = user_type.fields.get("name").expect("name field not found in User type");

            // With our implementation, the description from the first service should be used
            // This is an implementation detail that could change, so we're just checking that
            // a description exists rather than which one was chosen
            assert!(name_field.description.is_some(), "name field should have a description");
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}

#[test]
fn test_shareable_type_can_be_referenced_by_other_subgraphs() {
    // Define a subgraph that defines a shareable type
    let service1_sdl = r#"
    type Product @key(fields: "id") @shareable {
        id: ID!
        name: String!
        price: Int!
    }

    type Query {
        product(id: ID!): Product
    }
    "#;

    // Define a subgraph that references the shareable type
    let service2_sdl = r#"
    # Reference the Product type without redefining all its fields
    type Product @key(fields: "id") {
        id: ID!
        # Add a new field to the Product type
        reviews: [Review!]!
    }

    type Review {
        id: ID!
        text: String!
        rating: Int!
    }

    type Query {
        reviews(productId: ID!): [Review!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should succeed because Product is marked as @shareable
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination succeeded
    match result {
        Ok(schema) => {
            // Verify that the Product type exists and has all fields from both subgraphs
            let product_type = schema
                .types
                .get("Product")
                .expect("Product type not found in combined schema");

            // Check fields from service1
            assert!(
                product_type.fields.contains_key("name"),
                "name field not found in Product type"
            );
            assert!(
                product_type.fields.contains_key("price"),
                "price field not found in Product type"
            );

            // Check field from service2
            assert!(
                product_type.fields.contains_key("reviews"),
                "reviews field not found in Product type"
            );

            // Verify that the Review type exists
            assert!(
                schema.types.contains_key("Review"),
                "Review type not found in combined schema"
            );

            // Verify that the Product type doesn't have an owner since it's shareable
            assert!(
                product_type.owner.is_none(),
                "Shareable Product type should not have an owner"
            );
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}

#[test]
fn test_non_shareable_type_can_be_referenced_by_other_subgraphs_with_field_compatibility() {
    // Define a subgraph that defines a non-shareable type
    let service1_sdl = r#"
    type Product @key(fields: "id") {
        id: ID!
        name: String!
        price: Int!
    }

    type Query {
        product(id: ID!): Product
    }
    "#;

    // Define a subgraph that tries to reference the non-shareable type
    let service2_sdl = r#"
    # Try to reference the Product type without redefining all its fields
    type Product @key(fields: "id") {
        id: ID!
        # Add a new field to the Product type
        reviews: [Review!]!
    }

    type Review {
        id: ID!
        text: String!
        rating: Int!
    }

    type Query {
        reviews(productId: ID!): [Review!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should succeed because we're using field compatibility
    // In a real Federation v2 implementation, this might be considered an error
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination succeeded
    match result {
        Ok(schema) => {
            // Verify that the Product type exists and has all fields from both subgraphs
            let product_type = schema
                .types
                .get("Product")
                .expect("Product type not found in combined schema");

            // Check fields from service1
            assert!(
                product_type.fields.contains_key("name"),
                "name field not found in Product type"
            );
            assert!(
                product_type.fields.contains_key("price"),
                "price field not found in Product type"
            );

            // Check field from service2
            assert!(
                product_type.fields.contains_key("reviews"),
                "reviews field not found in Product type"
            );

            // Verify that the Review type exists
            assert!(
                schema.types.contains_key("Review"),
                "Review type not found in combined schema"
            );

            // With our current implementation, the type doesn't have an owner because it's referenced by multiple
            // services This is a deviation from the Federation v2 spec, but it's a convenience feature
            assert!(
                product_type.owner.is_none(),
                "Product type should not have an owner with our field compatibility feature"
            );
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}

#[test]
fn test_shareable_fields_with_deprecation() {
    // Define two services with the same field, one with deprecation
    let service1_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        oldField: String! @deprecated(reason: "Use newField instead") @shareable
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    let service2_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        oldField: String! @deprecated(reason: "Use newField instead") @shareable
        newField: String!
    }

    type Query {
        users: [User!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should succeed
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination succeeded
    match result {
        Ok(schema) => {
            // Verify that the User type exists and has the oldField field
            let user_type = schema
                .types
                .get("User")
                .expect("User type not found in combined schema");
            let old_field = user_type
                .fields
                .get("oldField")
                .expect("oldField not found in User type");

            // Verify that the oldField is deprecated
            assert!(old_field.deprecation.is_deprecated(), "oldField should be deprecated");
            assert_eq!(old_field.deprecation.reason(), Some("Use newField instead"));
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}

#[test]
fn test_three_services_with_compatible_fields() {
    // Define three services with the same field
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

    let service3_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String! @shareable
        age: Int!
    }

    type Query {
        usersByAge(age: Int!): [User!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");
    let service3_doc = parse_schema(service3_sdl).expect("Failed to parse service3 SDL");

    // Combine the services - this should succeed because 'name' is marked as @shareable
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
        ("service3".to_string(), service3_doc),
    ]);

    // Verify that the combination succeeded
    match result {
        Ok(schema) => {
            // Verify that the User type exists and has all fields from all services
            let user_type = schema
                .types
                .get("User")
                .expect("User type not found in combined schema");
            assert!(
                user_type.fields.contains_key("name"),
                "name field not found in User type"
            );
            assert!(
                user_type.fields.contains_key("email"),
                "email field not found in User type"
            );
            assert!(user_type.fields.contains_key("age"), "age field not found in User type");
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}

#[test]
fn test_incompatible_field_arguments_with_shareable_succeeds() {
    // Define two services with the same field with incompatible arguments but marked as @shareable
    let service1_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        posts(limit: Int = 10): [String!]! @shareable
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    let service2_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        posts(limit: Int = 10, offset: Int!): [String!]! @shareable  # Added required argument
        email: String!
    }

    type Query {
        users: [User!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should succeed because 'posts' is marked as @shareable
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination succeeded
    match result {
        Ok(schema) => {
            // Verify that the User type exists and has the posts field
            let user_type = schema
                .types
                .get("User")
                .expect("User type not found in combined schema");
            assert!(
                user_type.fields.contains_key("posts"),
                "posts field not found in User type"
            );
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}

#[test]
fn test_shareable_type_with_multiple_keys() {
    // Define a subgraph that defines a shareable type with multiple keys
    let service1_sdl = r#"
    type Product @key(fields: "id") @key(fields: "sku") @shareable {
        id: ID!
        sku: String!
        name: String!  # Not marked as @shareable
        price: Int! @shareable
    }

    type Query {
        product(id: ID!): Product
        productBySku(sku: String!): Product
    }
    "#;

    // Define a subgraph that references the shareable type using both keys
    let service2_sdl = r#"
    # Reference the Product type with both keys
    type Product @key(fields: "id") @key(fields: "sku") {
        id: ID!
        sku: String!
        # Add a new field to the Product type
        reviews: [Review!]!
        # Try to reference non-shareable field
        name: String! @external
        price: Int! @external
    }

    type Review {
        id: ID!
        text: String!
        rating: Int!
    }

    type Query {
        reviews(productId: ID!): [Review!]!
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

    // Verify that the combination failed with the expected error
    match result {
        Err(CombineError::FieldConflicted { type_name, field_name }) => {
            assert_eq!(type_name, "Product");
            assert_eq!(field_name, "name");
        },
        Ok(_) => panic!("Expected combination to fail due to non-shareable field"),
        Err(e) => panic!("Expected FieldConflicted error, got: {:?}", e),
    }
}

#[test]
fn test_non_shareable_type_cannot_be_referenced_by_other_subgraphs() {
    // Define a subgraph that defines a non-shareable type
    let service1_sdl = r#"
    type Product @key(fields: "id") {
        id: ID!
        name: String!
        price: Int!
    }

    type Query {
        product(id: ID!): Product
    }
    "#;

    // Define a subgraph that tries to reference the non-shareable type
    let service2_sdl = r#"
    # Try to reference the Product type without redefining all its fields
    type Product @key(fields: "id") {
        id: ID!
        # Add a new field to the Product type
        reviews: [Review!]!
        # Try to reference non-shareable fields
        name: String! @external
    }

    type Review {
        id: ID!
        text: String!
        rating: Int!
    }

    type Query {
        reviews(productId: ID!): [Review!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should fail because Product is not marked as @shareable
    // and 'name' is not marked as @shareable
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination failed
    match result {
        Err(CombineError::FieldConflicted { type_name, field_name }) => {
            assert_eq!(type_name, "Product");
            assert_eq!(field_name, "name");
        },
        Ok(_) => panic!("Expected combination to fail due to non-shareable field"),
        Err(e) => panic!("Expected FieldConflicted error, got: {:?}", e),
    }
}

#[test]
fn test_entity_key_fields_can_be_shared_without_shareable() {
    // Define two services with the same field that is part of an entity key
    let service1_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        email: String!
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    let service2_sdl = r#"
    type User @key(fields: "id") {
        id: ID!  # This is part of the entity key, so it can be shared without @shareable
        name: String!
    }

    type Query {
        users: [User!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should succeed because 'id' is part of the entity key
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination succeeded
    match result {
        Ok(schema) => {
            // Verify that the User type exists and has the id field
            let user_type = schema
                .types
                .get("User")
                .expect("User type not found in combined schema");
            assert!(user_type.fields.contains_key("id"), "id field not found in User type");

            // Verify that both services' fields are present
            assert!(
                user_type.fields.contains_key("email"),
                "email field not found in User type"
            );
            assert!(
                user_type.fields.contains_key("name"),
                "name field not found in User type"
            );
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}

#[test]
fn test_value_type_ownership_is_enforced() {
    // Define a subgraph that defines a value type (enum)
    let service1_sdl = r#"
    enum ProductCategory {
        CLOTHING
        ELECTRONICS
        BOOKS
    }

    type Product @key(fields: "id") {
        id: ID!
        name: String!
        category: ProductCategory!
    }

    type Query {
        product(id: ID!): Product
    }
    "#;

    // Define a subgraph that tries to redefine the same value type without @shareable
    let service2_sdl = r#"
    # Trying to redefine the enum without @shareable
    enum ProductCategory {
        CLOTHING
        ELECTRONICS
        BOOKS
    }

    type Review {
        id: ID!
        text: String!
        productCategory: ProductCategory!
    }

    type Query {
        reviews: [Review!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should fail because ProductCategory is not marked as @shareable
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination failed with a ValueTypeOwnershipConflicted error
    match result {
        Err(CombineError::ValueTypeOwnershipConflicted {
            type_name,
            owner_service,
            current_service,
        }) => {
            assert_eq!(type_name, "ProductCategory");
            assert_eq!(owner_service, "service1");
            assert_eq!(current_service, "service2");
        },
        Ok(_) => panic!("Expected combination to fail due to value type ownership conflict"),
        Err(e) => panic!("Expected ValueTypeOwnershipConflicted error, got: {:?}", e),
    }
}

#[test]
fn test_shareable_value_type_can_be_shared() {
    // Define a subgraph that defines a shareable value type (enum)
    let service1_sdl = r#"
    enum ProductCategory @shareable {
        CLOTHING
        ELECTRONICS
        BOOKS
    }

    type Product @key(fields: "id") {
        id: ID!
        name: String!
        category: ProductCategory!
    }

    type Query {
        product(id: ID!): Product
    }
    "#;

    // Define a subgraph that redefines the same value type with @shareable
    let service2_sdl = r#"
    # Redefining the enum with @shareable
    enum ProductCategory @shareable {
        CLOTHING
        ELECTRONICS
        BOOKS
    }

    type Review {
        id: ID!
        text: String!
        productCategory: ProductCategory!
    }

    type Query {
        reviews: [Review!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should succeed because ProductCategory is marked as @shareable
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination succeeded
    match result {
        Ok(schema) => {
            // Verify that the ProductCategory enum exists
            let product_category = schema
                .types
                .get("ProductCategory")
                .expect("ProductCategory not found in combined schema");

            // Verify that it has no owner since it's shareable
            assert!(
                product_category.owner.is_none(),
                "Shareable value type should not have an owner"
            );

            // Verify that it has the expected values
            assert_eq!(product_category.enum_values.len(), 3);
            assert!(product_category.enum_values.contains_key("CLOTHING"));
            assert!(product_category.enum_values.contains_key("ELECTRONICS"));
            assert!(product_category.enum_values.contains_key("BOOKS"));
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}

#[test]
fn test_common_scalar_types_can_be_shared_without_shareable() {
    // Define two services with the same common scalar type
    let service1_sdl = r#"
    scalar DateTime

    type Event @key(fields: "id") {
        id: ID!
        name: String!
        startTime: DateTime!
    }

    type Query {
        events: [Event!]!
    }
    "#;

    let service2_sdl = r#"
    scalar DateTime

    type Booking {
        id: ID!
        createdAt: DateTime!
    }

    type Query {
        bookings: [Booking!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should succeed because DateTime is a common scalar type
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination succeeded
    match result {
        Ok(schema) => {
            // Verify that the DateTime scalar exists
            let datetime_scalar = schema
                .types
                .get("DateTime")
                .expect("DateTime not found in combined schema");

            // Verify that it's a scalar type
            assert_eq!(datetime_scalar.kind, TypeKind::Scalar);
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}

#[test]
fn test_entity_types_can_be_referenced_across_subgraphs() {
    // Define a subgraph that defines an entity type
    let service1_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String!
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    // Define a subgraph that references the entity type
    let service2_sdl = r#"
    # Reference the User entity type without @shareable
    type User @key(fields: "id") {
        id: ID!
        email: String!
    }

    type Query {
        users: [User!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should succeed because User is an entity type
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination succeeded
    match result {
        Ok(schema) => {
            // Verify that the User type exists
            let user_type = schema
                .types
                .get("User")
                .expect("User type not found in combined schema");

            // Verify that it's marked as an entity
            assert!(user_type.is_entity(), "User should be marked as an entity");

            // Verify that it has fields from both services
            assert!(user_type.fields.contains_key("id"), "id field not found in User type");
            assert!(
                user_type.fields.contains_key("name"),
                "name field not found in User type"
            );
            assert!(
                user_type.fields.contains_key("email"),
                "email field not found in User type"
            );

            // Verify that it has keys for both services
            assert!(
                user_type.keys.contains_key("service1"),
                "User should have keys for service1"
            );
            assert!(
                user_type.keys.contains_key("service2"),
                "User should have keys for service2"
            );
        },
        Err(e) => panic!("Expected combination to succeed, got error: {:?}", e),
    }
}

#[test]
fn test_value_types_cannot_be_referenced_across_subgraphs_without_shareable() {
    // Define a subgraph that defines a value type (interface)
    let service1_sdl = r#"
    interface Node {
        id: ID!
    }

    type User implements Node @key(fields: "id") {
        id: ID!
        name: String!
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    // Define a subgraph that tries to reference the value type without @shareable
    let service2_sdl = r#"
    # Try to reference the Node interface without @shareable
    interface Node {
        id: ID!
    }

    type Product implements Node {
        id: ID!
        name: String!
        price: Int!
    }

    type Query {
        products: [Product!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service1_doc = parse_schema(service1_sdl).expect("Failed to parse service1 SDL");
    let service2_doc = parse_schema(service2_sdl).expect("Failed to parse service2 SDL");

    // Combine the services - this should fail because Node is a value type without @shareable
    let result = ComposedSchema::combine([
        ("service1".to_string(), service1_doc),
        ("service2".to_string(), service2_doc),
    ]);

    // Verify that the combination failed with a ValueTypeOwnershipConflicted error
    match result {
        Err(CombineError::ValueTypeOwnershipConflicted {
            type_name,
            owner_service,
            current_service,
        }) => {
            assert_eq!(type_name, "Node");
            assert_eq!(owner_service, "service1");
            assert_eq!(current_service, "service2");
        },
        Ok(_) => panic!("Expected combination to fail due to value type ownership conflict"),
        Err(e) => panic!("Expected ValueTypeOwnershipConflicted error, got: {:?}", e),
    }
}
