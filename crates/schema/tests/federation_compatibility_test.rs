use graphgate_schema::ComposedSchema;
use parser::parse_schema;
use tracing::debug;
use value::Name;

#[test]
fn test_combine_federation_v1_and_v2_services() {
    // Federation v1 schema (no @link directive)
    let v1_schema_str = r#"
    type Product @key(fields: "id") {
      id: ID!
      name: String!
      price: Int!
    }

    type Query {
      products: [Product!]!
    }
    "#;

    // Federation v2 schema (with @link directive)
    let v2_schema_str = r#"
    type Review @key(fields: "id") {
      id: ID!
      text: String!
      rating: Int!
      product: Product!
    }

    type Product @key(fields: "id") @shareable {
      id: ID!
      reviews: [Review!]!
    }

    type Query {
      reviews: [Review!]!
    }

    extend schema
      @link(
        url: "https://specs.apollo.dev/federation/v2.3"
        import: [
          "@key",
          "@shareable",
          "@provides"
        ]
      )
    "#;

    let v1_schema_doc = parse_schema(v1_schema_str).expect("Failed to parse v1 schema");
    let v2_schema_doc = parse_schema(v2_schema_str).expect("Failed to parse v2 schema");

    // Combine the schemas
    let combined_schema = ComposedSchema::combine([
        ("ProductsService".to_string(), v1_schema_doc),
        ("ReviewsService".to_string(), v2_schema_doc),
    ]);

    // Verify the schemas can be combined successfully
    assert!(
        combined_schema.is_ok(),
        "Failed to combine Federation v1 and v2 schemas: {:?}",
        combined_schema.err()
    );

    let schema = combined_schema.unwrap();

    // Verify the Product type exists and has fields from both services
    let product_type = schema.types.get(&Name::new("Product")).expect("Product type not found");

    // Check that the Product type has fields from both services
    assert!(
        product_type.fields.contains_key(&Name::new("id")),
        "Product.id field not found"
    );
    assert!(
        product_type.fields.contains_key(&Name::new("name")),
        "Product.name field not found"
    );
    assert!(
        product_type.fields.contains_key(&Name::new("price")),
        "Product.price field not found"
    );
    assert!(
        product_type.fields.contains_key(&Name::new("reviews")),
        "Product.reviews field not found"
    );

    // Verify the Query type has fields from both services
    let query_type = schema.types.get(&Name::new("Query")).expect("Query type not found");
    assert!(
        query_type.fields.contains_key(&Name::new("products")),
        "Query.products field not found"
    );
    assert!(
        query_type.fields.contains_key(&Name::new("reviews")),
        "Query.reviews field not found"
    );
}

#[test]
fn test_federation_v1_v2_field_sharing_compatibility() {
    // Federation v1 schema (no @shareable directive)
    let federation_v1_schema = r#"
        type Query {
            users: [User]
        }
        
        type User @key(fields: "id") {
            id: ID!
            name: String!
            email: String!
        }
    "#;

    // Federation v2 schema with @shareable directive
    let federation_v2_schema = r#"
        extend schema
          @link(
            url: "https://specs.apollo.dev/federation/v2.3"
            import: [
              "@key",
              "@shareable"
            ]
          )

        type Query {
            userProfiles: [User]
        }
        
        type User @key(fields: "id") {
            id: ID!
            profile: Profile!
        }
        
        type Profile {
            bio: String
            avatar: String
        }
    "#;

    let v1_schema_doc = parse_schema(federation_v1_schema).expect("Failed to parse v1 schema");
    let v2_schema_doc = parse_schema(federation_v2_schema).expect("Failed to parse v2 schema");

    // Combine the schemas
    let combined_schema = ComposedSchema::combine([
        ("UsersService".to_string(), v1_schema_doc),
        ("ProfilesService".to_string(), v2_schema_doc),
    ]);

    debug!("Combined schema result: {:?}", combined_schema);

    // Check if the combination was successful
    assert!(combined_schema.is_ok(), "Expected schemas to combine successfully");

    let schema = combined_schema.unwrap();

    // Check if the User type exists in the combined schema
    let user_type = schema
        .types
        .get(&Name::new("User"))
        .expect("User type should exist in combined schema");

    // Check if the fields from both services exist in the User type
    assert!(
        user_type.fields.contains_key(&Name::new("id")),
        "User.id field not found"
    );
    assert!(
        user_type.fields.contains_key(&Name::new("name")),
        "User.name field not found"
    );
    assert!(
        user_type.fields.contains_key(&Name::new("email")),
        "User.email field not found"
    );
    assert!(
        user_type.fields.contains_key(&Name::new("profile")),
        "User.profile field not found"
    );

    // This test demonstrates that:
    // 1. Federation v1 and v2 services can be combined successfully
    // 2. Each service can define its own fields on the same entity type
    // 3. As long as there are no field conflicts, the schemas can be combined
}

#[test]
fn test_federation_v1_v2_compatibility_with_shareable_fields() {
    // Federation v1 schema (no @link directive)
    let v1_schema_str = r#"
    type User @key(fields: "id") {
      id: ID!
      name: String!
      email: String!
    }

    type Query {
      users: [User!]!
    }
    "#;

    // Modified Federation v2 schema with no field conflicts
    let v2_schema_str = r#"
    extend schema
      @link(
        url: "https://specs.apollo.dev/federation/v2.3"
        import: [
          "@key",
          "@shareable"
        ]
      )

    type User @key(fields: "id") {
      id: ID!
      profile: Profile!
    }

    type Profile {
      bio: String
      avatar: String
    }

    type Query {
      userProfiles: [User!]!
    }
    "#;

    let v1_schema_doc = parse_schema(v1_schema_str).expect("Failed to parse v1 schema");
    let v2_schema_doc = parse_schema(v2_schema_str).expect("Failed to parse v2 schema");

    // Combine the schemas
    let combined_schema = ComposedSchema::combine([
        ("UsersService".to_string(), v1_schema_doc),
        ("ProfilesService".to_string(), v2_schema_doc),
    ]);

    // This should succeed because there are no field conflicts
    assert!(
        combined_schema.is_ok(),
        "Failed to combine schemas: {:?}",
        combined_schema.err()
    );

    let schema = combined_schema.unwrap();

    // Verify the User type has fields from both services
    let user_type = schema.types.get(&Name::new("User")).expect("User type not found");
    assert!(
        user_type.fields.contains_key(&Name::new("id")),
        "User.id field not found"
    );
    assert!(
        user_type.fields.contains_key(&Name::new("name")),
        "User.name field not found"
    );
    assert!(
        user_type.fields.contains_key(&Name::new("email")),
        "User.email field not found"
    );
    assert!(
        user_type.fields.contains_key(&Name::new("profile")),
        "User.profile field not found"
    );
}
