use graphgate_schema::ComposedSchema;
use tracing_subscriber::EnvFilter;

#[test]
fn test() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("trace"))
                .unwrap(),
        )
        .init();
    let collections_service_document = parser::parse_schema(include_str!("collections.graphql")).unwrap();
    let collectibles_service_document = parser::parse_schema(include_str!("collectibles.graphql")).unwrap();
    let schema = ComposedSchema::combine([
        ("Collections".to_string(), collections_service_document),
        ("Collectibles".to_string(), collectibles_service_document),
    ])
    .unwrap();
    let query = schema.types.get(&schema.query_type.unwrap()).unwrap();
    dbg!(&query);
    assert!(!query.fields.is_empty());
}
