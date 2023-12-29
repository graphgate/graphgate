use graphgate_schema::ComposedSchema;
use tracing_subscriber::EnvFilter;
use value::Variables;

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
    let document = parser::parse_query(include_str!("collectibles_all.txt")).unwrap();
    let rule_errors = graphgate_validation::check_rules(&schema, &document, &Variables::default());
    dbg!(&rule_errors);
    assert!(rule_errors.is_empty());
}
