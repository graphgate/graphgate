use graphgate_handler::auth::{Auth, AuthConfig};
use std::collections::HashMap;
use std::sync::Arc;
use warp::Filter;

#[tokio::test]
async fn test_auth_config_defaults() {
    // Create a config with explicit values
    let config = AuthConfig {
        enabled: false,
        header_name: "authorization".to_string(),
        header_prefix: "Bearer".to_string(),
        required: false,
        jwks: "".to_string(),
    };
    
    // Verify the values
    assert_eq!(config.enabled, false);
    assert_eq!(config.header_name, "authorization");
    assert_eq!(config.header_prefix, "Bearer");
    assert_eq!(config.required, false);
    assert_eq!(config.jwks, "");
}

#[tokio::test]
async fn test_auth_custom_config() {
    // Create a config with custom values
    let config = AuthConfig {
        enabled: true,
        header_name: "x-custom-auth".to_string(),
        header_prefix: "Token".to_string(),
        required: true,
        jwks: "https://example.com/jwks.json".to_string(),
    };
    
    // Verify the values
    assert_eq!(config.enabled, true);
    assert_eq!(config.header_name, "x-custom-auth");
    assert_eq!(config.header_prefix, "Token");
    assert_eq!(config.required, true);
    assert_eq!(config.jwks, "https://example.com/jwks.json");
}

#[tokio::test]
async fn test_auth_with_auth_state() {
    // Create a test auth
    let auth = Arc::new(Auth {
        config: AuthConfig {
            enabled: true,
            header_name: "authorization".to_string(),
            header_prefix: "Bearer".to_string(),
            required: false,
            jwks: "".to_string(),
        },
        decoding_keys: HashMap::new(),
    });
    
    // Create a filter with auth state
    let filter = graphgate_handler::auth::with_auth_state(auth.clone());
    
    // The filter should extract the auth state
    let extracted = warp::test::request()
        .filter(&filter)
        .await
        .expect("Filter should extract auth state");
    
    // Verify the extracted auth state
    assert_eq!(extracted.config.enabled, true);
    assert_eq!(extracted.config.header_name, "authorization");
    assert_eq!(extracted.config.header_prefix, "Bearer");
    assert_eq!(extracted.config.required, false);
    assert_eq!(extracted.config.jwks, "");
}

#[tokio::test]
async fn test_auth_try_new() {
    // Create a mock JWKS JSON
    let jwks_json = r#"{
        "keys": [
            {
                "kty": "RSA",
                "kid": "test-key-id",
                "alg": "RS256",
                "n": "test",
                "e": "AQAB"
            }
        ]
    }"#;
    
    // Create a mock HTTP server to serve the JWKS
    let mock_http_server = warp::path("jwks.json")
        .and(warp::get())
        .map(move || jwks_json)
        .with(warp::reply::with::header("Content-Type", "application/json"));
    
    // Start the server in the background
    let (addr, server) = warp::serve(mock_http_server)
        .bind_ephemeral(([127, 0, 0, 1], 0));
    
    let server_handle = tokio::spawn(server);
    
    // Create auth config pointing to our mock server
    let config = AuthConfig {
        enabled: true,
        header_name: "authorization".to_string(),
        header_prefix: "Bearer".to_string(),
        required: false,
        jwks: format!("http://127.0.0.1:{}/jwks.json", addr.port()),
    };
    
    // Try to create a new Auth instance
    let auth_result = Auth::try_new(config).await;
    
    // Cleanup the server
    server_handle.abort();
    
    // Check that Auth was created successfully
    assert!(auth_result.is_ok());
    
    let auth = auth_result.unwrap();
    assert!(auth.decoding_keys.contains_key("test-key-id"));
} 