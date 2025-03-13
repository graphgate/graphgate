use graphgate_handler::{
    auth::{Auth, AuthConfig},
    handler::{graphql_request, HandlerConfig},
    SharedRouteTable,
};
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;
use warp::{http::StatusCode, test::request};

#[tokio::test]
async fn test_multiple_queries_in_same_request() {
    // Initialize a minimal SharedRouteTable for testing
    let shared_route_table = SharedRouteTable::default();

    // Create a handler config
    let config = HandlerConfig {
        shared_route_table: shared_route_table.clone(),
        forward_headers: Arc::new(vec![]),
    };

    // Create a test auth
    let auth = Arc::new(Auth {
        config: AuthConfig::default(),
        decoding_keys: Default::default(),
    });

    // Create the GraphQL endpoint
    let api = graphql_request(auth, config);

    // Test 1: Single query (should work)
    let single_query = r#"{
        "query": "query { dog { name } }"
    }"#;

    let resp = request()
        .method("POST")
        .path("/")
        .header("content-type", "application/json")
        .body(single_query)
        .reply(&api)
        .await;

    // The response might be an error due to missing schema/services, but it shouldn't crash
    debug!("Single query response status: {:?}", resp.status());
    debug!("Single query response body: {}", String::from_utf8_lossy(resp.body()));
    assert!(resp.status() != StatusCode::INTERNAL_SERVER_ERROR);

    // Test 2: Multiple queries in the same request (the problematic case)
    let multiple_queries = r#"[
        {"query": "query { dog { name } }"},
        {"query": "query { cat { name } }"}
    ]"#;

    let resp = request()
        .method("POST")
        .path("/")
        .header("content-type", "application/json")
        .body(multiple_queries)
        .reply(&api)
        .await;

    // Check if the service handled the request without crashing
    debug!("Multiple queries response status: {:?}", resp.status());
    debug!(
        "Multiple queries response body: {}",
        String::from_utf8_lossy(resp.body())
    );

    // If the service doesn't support multiple queries, it should return a 400 Bad Request
    // If it does support them, it should return a 200 OK
    // But it should not crash with a 500 Internal Server Error
    assert!(resp.status() != StatusCode::INTERNAL_SERVER_ERROR);

    // Parse the response body to check the error message
    let response_body: Value = serde_json::from_slice(resp.body()).unwrap_or_default();
    debug!("Parsed response: {:?}", response_body);

    // Test 3: Multiple operations in a single query (should work with operation name)
    let multi_operation_query = r#"{
        "query": "query GetDog { dog { name } } query GetCat { cat { name } }",
        "operation": "GetDog"
    }"#;

    let resp = request()
        .method("POST")
        .path("/")
        .header("content-type", "application/json")
        .body(multi_operation_query)
        .reply(&api)
        .await;

    // Check if the service handled the request without crashing
    debug!("Multi-operation query response status: {:?}", resp.status());
    debug!(
        "Multi-operation query response body: {}",
        String::from_utf8_lossy(resp.body())
    );
    assert!(resp.status() != StatusCode::INTERNAL_SERVER_ERROR);
}
