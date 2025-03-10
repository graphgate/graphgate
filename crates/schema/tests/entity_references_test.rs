use graphgate_schema::ComposedSchema;
use parser::parse_schema;

#[test]
fn test_nested_entity_references() {
    // Define Service A with User and Profile entities
    let service_a_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String! @shareable
        profile: Profile!
    }

    type Profile @key(fields: "id") {
        id: ID!
        bio: String! @shareable
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    // Define Service B with User and Profile entities with additional fields
    let service_b_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String! @shareable
        posts: [Post!]!
    }

    type Post @key(fields: "id") {
        id: ID!
        title: String!
        author: User!
    }

    type Profile @key(fields: "id") {
        id: ID!
        bio: String! @shareable
        avatar: String!
    }

    type Query {
        posts: [Post!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service_a_doc = parse_schema(service_a_sdl).expect("Failed to parse service A SDL");
    let service_b_doc = parse_schema(service_b_sdl).expect("Failed to parse service B SDL");

    // Combine the services - this should succeed because the shared fields are marked as @shareable
    let result = ComposedSchema::combine([
        ("ServiceA".to_string(), service_a_doc),
        ("ServiceB".to_string(), service_b_doc),
    ]);

    // Verify that the combination succeeded
    assert!(
        result.is_ok(),
        "Expected combination to succeed, got error: {:?}",
        result.err()
    );

    let schema = result.unwrap();

    // Verify that the User type has all fields from both services
    let user_type = schema
        .types
        .get("User")
        .expect("User type not found in combined schema");
    assert!(user_type.fields.contains_key("id"), "id field not found in User type");
    assert!(
        user_type.fields.contains_key("name"),
        "name field not found in User type"
    );
    assert!(
        user_type.fields.contains_key("profile"),
        "profile field not found in User type"
    );
    assert!(
        user_type.fields.contains_key("posts"),
        "posts field not found in User type"
    );

    // Verify that the Profile type has all fields from both services
    let profile_type = schema
        .types
        .get("Profile")
        .expect("Profile type not found in combined schema");
    assert!(
        profile_type.fields.contains_key("id"),
        "id field not found in Profile type"
    );
    assert!(
        profile_type.fields.contains_key("bio"),
        "bio field not found in Profile type"
    );
    assert!(
        profile_type.fields.contains_key("avatar"),
        "avatar field not found in Profile type"
    );
}

#[test]
fn test_entity_references_with_lists() {
    // Define Service A with Product entity
    let service_a_sdl = r#"
    type Product @key(fields: "id") {
        id: ID!
        name: String! @shareable
        price: Int! @shareable
    }

    type Query {
        product(id: ID!): Product
    }
    "#;

    // Define Service B with Order entity referencing Product
    let service_b_sdl = r#"
    type Order @key(fields: "id") {
        id: ID!
        products: [Product!]!
        total: Int!
    }

    type Product @key(fields: "id") {
        id: ID!
        name: String! @shareable
        price: Int! @shareable
    }

    type Query {
        order(id: ID!): Order
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service_a_doc = parse_schema(service_a_sdl).expect("Failed to parse service A SDL");
    let service_b_doc = parse_schema(service_b_sdl).expect("Failed to parse service B SDL");

    // Combine the services - this should succeed because the shared fields are marked as @shareable
    let result = ComposedSchema::combine([
        ("ServiceA".to_string(), service_a_doc),
        ("ServiceB".to_string(), service_b_doc),
    ]);

    // Verify that the combination succeeded
    assert!(
        result.is_ok(),
        "Expected combination to succeed, got error: {:?}",
        result.err()
    );

    let schema = result.unwrap();

    // Verify that the Product type has all fields
    let product_type = schema
        .types
        .get("Product")
        .expect("Product type not found in combined schema");
    assert!(
        product_type.fields.contains_key("id"),
        "id field not found in Product type"
    );
    assert!(
        product_type.fields.contains_key("name"),
        "name field not found in Product type"
    );
    assert!(
        product_type.fields.contains_key("price"),
        "price field not found in Product type"
    );

    // Verify that the Order type exists and has the products field
    let order_type = schema
        .types
        .get("Order")
        .expect("Order type not found in combined schema");
    assert!(
        order_type.fields.contains_key("products"),
        "products field not found in Order type"
    );
}

#[test]
fn test_entity_references_with_interface_types() {
    // Define Service A with Node interface and User entity
    let service_a_sdl = r#"
    interface Node @shareable {
        id: ID!
    }

    type User implements Node @key(fields: "id") {
        id: ID!
        name: String! @shareable
    }

    type Query {
        user(id: ID!): User @shareable
    }
    "#;

    // Define Service B with Node interface and User entity with additional fields
    let service_b_sdl = r#"
    interface Node @shareable {
        id: ID!
    }

    type User implements Node @key(fields: "id") {
        id: ID!
        name: String! @shareable
        email: String!
    }

    type Query {
        node(id: ID!): Node
        user(id: ID!): User @shareable
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service_a_doc = parse_schema(service_a_sdl).expect("Failed to parse service A SDL");
    let service_b_doc = parse_schema(service_b_sdl).expect("Failed to parse service B SDL");

    // Combine the services - this should succeed because the shared fields are marked as @shareable
    let result = ComposedSchema::combine([
        ("ServiceA".to_string(), service_a_doc),
        ("ServiceB".to_string(), service_b_doc),
    ]);

    // Verify that the combination succeeded
    assert!(
        result.is_ok(),
        "Expected combination to succeed, got error: {:?}",
        result.err()
    );

    let schema = result.unwrap();

    // Verify that the Node interface exists
    let node_interface = schema
        .types
        .get("Node")
        .expect("Node interface not found in combined schema");
    assert!(
        node_interface.fields.contains_key("id"),
        "id field not found in Node interface"
    );

    // Verify that the User type implements Node and has all fields
    let user_type = schema
        .types
        .get("User")
        .expect("User type not found in combined schema");
    assert!(
        user_type.implements.contains("Node"),
        "User type does not implement Node interface"
    );
    assert!(user_type.fields.contains_key("id"), "id field not found in User type");
    assert!(
        user_type.fields.contains_key("name"),
        "name field not found in User type"
    );
    assert!(
        user_type.fields.contains_key("email"),
        "email field not found in User type"
    );
}

#[test]
fn test_entity_references_with_union_types() {
    // Define Service A with User and Product entities
    let service_a_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String! @shareable
    }

    type Product @key(fields: "id") {
        id: ID!
        name: String! @shareable
    }

    type Query {
        user(id: ID!): User
        product(id: ID!): Product
    }
    "#;

    // Define Service B with User and Product entities and a SearchResult union
    let service_b_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String! @shareable
    }

    type Product @key(fields: "id") {
        id: ID!
        name: String! @shareable
    }

    union SearchResult = User | Product

    type Query {
        search(query: String!): [SearchResult!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service_a_doc = parse_schema(service_a_sdl).expect("Failed to parse service A SDL");
    let service_b_doc = parse_schema(service_b_sdl).expect("Failed to parse service B SDL");

    // Combine the services - this should succeed because the shared fields are marked as @shareable
    let result = ComposedSchema::combine([
        ("ServiceA".to_string(), service_a_doc),
        ("ServiceB".to_string(), service_b_doc),
    ]);

    // Verify that the combination succeeded
    assert!(
        result.is_ok(),
        "Expected combination to succeed, got error: {:?}",
        result.err()
    );

    let schema = result.unwrap();

    // Verify that the SearchResult union exists and includes User and Product
    let search_result_union = schema
        .types
        .get("SearchResult")
        .expect("SearchResult union not found in combined schema");
    assert!(
        search_result_union.possible_types.contains("User"),
        "User type not included in SearchResult union"
    );
    assert!(
        search_result_union.possible_types.contains("Product"),
        "Product type not included in SearchResult union"
    );
}

#[test]
fn test_entity_references_with_arguments() {
    // Define Service A with User entity having a field with arguments
    let service_a_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name(uppercase: Boolean = false): String! @shareable
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    // Define Service B with User entity having the same field with arguments
    let service_b_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name(uppercase: Boolean = false): String! @shareable
        posts(limit: Int = 10): [Post!]!
    }

    type Post {
        id: ID!
        title: String!
    }

    type Query {
        posts: [Post!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service_a_doc = parse_schema(service_a_sdl).expect("Failed to parse service A SDL");
    let service_b_doc = parse_schema(service_b_sdl).expect("Failed to parse service B SDL");

    // Combine the services - this should succeed because the shared fields are marked as @shareable
    let result = ComposedSchema::combine([
        ("ServiceA".to_string(), service_a_doc),
        ("ServiceB".to_string(), service_b_doc),
    ]);

    // Verify that the combination succeeded
    assert!(
        result.is_ok(),
        "Expected combination to succeed, got error: {:?}",
        result.err()
    );

    let schema = result.unwrap();

    // Verify that the User type has the name field with the uppercase argument
    let user_type = schema
        .types
        .get("User")
        .expect("User type not found in combined schema");
    let name_field = user_type.fields.get("name").expect("name field not found in User type");
    assert!(
        name_field.arguments.contains_key("uppercase"),
        "uppercase argument not found in name field"
    );

    // Verify that the User type has the posts field with the limit argument
    let posts_field = user_type
        .fields
        .get("posts")
        .expect("posts field not found in User type");
    assert!(
        posts_field.arguments.contains_key("limit"),
        "limit argument not found in posts field"
    );
}

#[test]
fn test_entity_references_with_multiple_key_fields() {
    // Define Service A with Product entity having multiple key fields
    let service_a_sdl = r#"
    type Product @key(fields: "id") @key(fields: "sku") {
        id: ID!
        sku: String!
        name: String! @shareable
        price: Int! @shareable
    }

    type Query {
        product(id: ID!): Product
        productBySku(sku: String!): Product
    }
    "#;

    // Define Service B with Product entity referencing by id
    let service_b_sdl = r#"
    type Product @key(fields: "id") {
        id: ID!
        name: String! @shareable
        price: Int! @shareable
        description: String!
    }

    type Order {
        id: ID!
        product: Product!
    }

    type Query {
        order(id: ID!): Order
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service_a_doc = parse_schema(service_a_sdl).expect("Failed to parse service A SDL");
    let service_b_doc = parse_schema(service_b_sdl).expect("Failed to parse service B SDL");

    // Combine the services - this should succeed because the shared fields are marked as @shareable
    let result = ComposedSchema::combine([
        ("ServiceA".to_string(), service_a_doc),
        ("ServiceB".to_string(), service_b_doc),
    ]);

    // Verify that the combination succeeded
    assert!(
        result.is_ok(),
        "Expected combination to succeed, got error: {:?}",
        result.err()
    );

    let schema = result.unwrap();

    // Verify that the Product type has all fields from both services
    let product_type = schema
        .types
        .get("Product")
        .expect("Product type not found in combined schema");
    assert!(
        product_type.fields.contains_key("id"),
        "id field not found in Product type"
    );
    assert!(
        product_type.fields.contains_key("sku"),
        "sku field not found in Product type"
    );
    assert!(
        product_type.fields.contains_key("name"),
        "name field not found in Product type"
    );
    assert!(
        product_type.fields.contains_key("price"),
        "price field not found in Product type"
    );
    assert!(
        product_type.fields.contains_key("description"),
        "description field not found in Product type"
    );

    // Verify that the Product type has both key fields
    assert!(product_type.is_entity(), "Product type is not marked as an entity");
    assert!(!product_type.keys.is_empty(), "Product type has no key fields");
}

#[test]
fn test_circular_entity_references() {
    // Define Service A with User entity referencing itself
    let service_a_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String! @shareable
        bestFriend: User
    }

    type Query {
        user(id: ID!): User
    }
    "#;

    // Define Service B with User entity and Post entity referencing User
    let service_b_sdl = r#"
    type User @key(fields: "id") {
        id: ID!
        name: String! @shareable
        posts: [Post!]!
    }

    type Post @key(fields: "id") {
        id: ID!
        title: String!
        author: User!
    }

    type Query {
        posts: [Post!]!
    }
    "#;

    // Parse the SDL into ServiceDocuments
    let service_a_doc = parse_schema(service_a_sdl).expect("Failed to parse service A SDL");
    let service_b_doc = parse_schema(service_b_sdl).expect("Failed to parse service B SDL");

    // Combine the services - this should succeed because the shared fields are marked as @shareable
    let result = ComposedSchema::combine([
        ("ServiceA".to_string(), service_a_doc),
        ("ServiceB".to_string(), service_b_doc),
    ]);

    // Verify that the combination succeeded
    assert!(
        result.is_ok(),
        "Expected combination to succeed, got error: {:?}",
        result.err()
    );

    let schema = result.unwrap();

    // Verify that the User type has all fields from both services
    let user_type = schema
        .types
        .get("User")
        .expect("User type not found in combined schema");
    assert!(user_type.fields.contains_key("id"), "id field not found in User type");
    assert!(
        user_type.fields.contains_key("name"),
        "name field not found in User type"
    );
    assert!(
        user_type.fields.contains_key("bestFriend"),
        "bestFriend field not found in User type"
    );
    assert!(
        user_type.fields.contains_key("posts"),
        "posts field not found in User type"
    );

    // Verify that the Post type exists and has the author field
    let post_type = schema
        .types
        .get("Post")
        .expect("Post type not found in combined schema");
    assert!(
        post_type.fields.contains_key("author"),
        "author field not found in Post type"
    );
}
