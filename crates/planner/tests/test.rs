use std::fs;

use globset::GlobBuilder;
use graphgate_planner::PlanBuilder;
use graphgate_schema::ComposedSchema;
use pretty_assertions::assert_eq;
use tracing::debug;
use tracing_subscriber::EnvFilter;
use value::Name;

#[test]
fn test() {
    let schema = ComposedSchema::parse(include_str!("test.graphql")).unwrap();
    let glob = GlobBuilder::new("./tests/*.txt")
        .literal_separator(true)
        .build()
        .unwrap()
        .compile_matcher();

    for entry in fs::read_dir("./tests").unwrap() {
        let entry = entry.unwrap();
        if !glob.is_match(entry.path()) {
            continue;
        }

        println!("{}", entry.path().display());

        let data = fs::read_to_string(entry.path()).unwrap();
        let mut s = data.split("---");
        let mut n = 1;

        loop {
            println!("\tIndex: {}", n);
            let graphql = match s.next() {
                Some(graphql) => graphql,
                None => break,
            };
            let variables = s.next().unwrap();
            let planner_json = s.next().unwrap();

            let document = parser::parse_query(graphql).unwrap();
            let builder = PlanBuilder::new(&schema, document).variables(serde_json::from_str(variables).unwrap());
            let expect_node: serde_json::Value = serde_json::from_str(planner_json).unwrap();
            let actual_node = serde_json::to_value(builder.plan().unwrap()).unwrap();

            assert_eq!(actual_node, expect_node);

            n += 1;
        }
    }
}

#[test]
fn test_federation() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("debug"))
                .unwrap(),
        )
        .init();
    let collections_service_document = parser::parse_schema(include_str!("collections.graphql")).unwrap();
    let collectibles_service_document = parser::parse_schema(include_str!("collectibles.graphql")).unwrap();
    let data = fs::read_to_string("./tests/federation.text").unwrap();
    let mut s = data.split("---");
    let graphql = s.next().unwrap();
    let variables = s.next().unwrap();
    let planner_json = s.next().unwrap();
    let schema = ComposedSchema::combine([
        ("collectibles".to_string(), collectibles_service_document.clone()),
        ("collections".to_string(), collections_service_document.clone()),
    ])
    .unwrap();
    let reverse_order_schema = ComposedSchema::combine([
        ("collections".to_string(), collections_service_document),
        ("collectibles".to_string(), collectibles_service_document),
    ])
    .unwrap();
    let document = parser::parse_query(graphql).unwrap();
    assert_eq!(schema.query_type, reverse_order_schema.query_type);
    assert_eq!(schema.mutation_type, reverse_order_schema.mutation_type);
    assert_eq!(schema.subscription_type, reverse_order_schema.subscription_type);
    assert_eq!(schema.directives, reverse_order_schema.directives);
    assert_eq!(
        schema.types.get(&Name::new("Query")),
        reverse_order_schema.types.get(&Name::new("Query")),
    );
    debug!("One order");
    {
        // One order
        let builder = PlanBuilder::new(&schema, document.clone()).variables(serde_json::from_str(variables).unwrap());
        let expect_node: serde_json::Value = serde_json::from_str(planner_json).unwrap();
        let actual_node = serde_json::to_value(builder.plan().unwrap()).unwrap();
        assert_eq!(actual_node, expect_node);
    }
    debug!("Reverse order");
    {
        // Reverse order
        let builder =
            PlanBuilder::new(&reverse_order_schema, document).variables(serde_json::from_str(variables).unwrap());
        let expect_node: serde_json::Value = serde_json::from_str(planner_json).unwrap();
        let actual_node = serde_json::to_value(builder.plan().unwrap()).unwrap();
        assert_eq!(actual_node, expect_node);
    }
}
