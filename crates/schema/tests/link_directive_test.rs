use graphgate_schema::ComposedSchema;
use parser::parse_schema;

#[test]
fn test_link_directive_federation_v2() {
    // Test schema with @link directive
    let schema_str = r#"
    type Product @key(fields: "id") {
      id: ID!
      name: String!
      price: Int!
    }

    type Query {
      products: [Product!]!
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

    let schema_doc = parse_schema(schema_str).expect("Failed to parse schema");
    let schema = ComposedSchema::new(schema_doc);

    // Verify Federation version was detected
    assert!(schema.is_federation_v2());
    assert_eq!(schema.federation_version, Some("2.3".to_string()));

    // Verify directive mapping
    assert!(schema.is_directive_imported("key"));
    assert!(schema.is_directive_imported("shareable"));
    assert!(schema.is_directive_imported("provides"));
    assert!(!schema.is_directive_imported("external"));

    // Verify namespace
    assert_eq!(schema.federation_namespace, Some("federation__".to_string()));
    assert_eq!(schema.get_namespaced_directive("external"), "federation__external");
}

#[test]
fn test_link_directive_with_custom_namespace() {
    // Test schema with @link directive and custom namespace
    let schema_str = r#"
    type Product @key(fields: "id") {
      id: ID!
      name: String!
      price: Int!
    }

    type Query {
      products: [Product!]!
    }

    extend schema
      @link(
        url: "https://specs.apollo.dev/federation/v2.3"
        as: "fed"
        import: [
          "@key"
        ]
      )
    "#;

    let schema_doc = parse_schema(schema_str).expect("Failed to parse schema");
    let schema = ComposedSchema::new(schema_doc);

    // Verify Federation version was detected
    assert!(schema.is_federation_v2());
    assert_eq!(schema.federation_version, Some("2.3".to_string()));

    // Verify directive mapping
    assert!(schema.is_directive_imported("key"));
    assert!(!schema.is_directive_imported("shareable"));

    // Verify custom namespace
    assert_eq!(schema.federation_namespace, Some("fed__".to_string()));
    assert_eq!(schema.get_namespaced_directive("shareable"), "fed__shareable");
}

#[test]
fn test_link_directive_with_renamed_directives() {
    // Test schema with @link directive and renamed directives
    let schema_str = r#"
    type Product @uniqueKey(fields: "id") {
      id: ID!
      name: String!
      price: Int!
    }

    type Query {
      products: [Product!]!
    }

    extend schema
      @link(
        url: "https://specs.apollo.dev/federation/v2.3"
        import: [
          { name: "@key", as: "@uniqueKey" }
        ]
      )
    "#;

    let schema_doc = parse_schema(schema_str).expect("Failed to parse schema");
    let schema = ComposedSchema::new(schema_doc);

    // Verify Federation version was detected
    assert!(schema.is_federation_v2());

    // Verify directive mapping
    assert!(schema.is_directive_imported("uniqueKey"));
    assert_eq!(schema.get_original_directive_name("uniqueKey"), "key".to_string());
}

#[test]
fn test_federation_v1_no_link_directive() {
    // Test schema without @link directive (Federation v1)
    let schema_str = r#"
    type Product @key(fields: "id") {
      id: ID!
      name: String!
      price: Int!
    }

    type Query {
      products: [Product!]!
    }
    "#;

    let schema_doc = parse_schema(schema_str).expect("Failed to parse schema");
    let schema = ComposedSchema::new(schema_doc);

    // Verify Federation version was not detected
    assert!(!schema.is_federation_v2());
    assert_eq!(schema.federation_version, None);
} 