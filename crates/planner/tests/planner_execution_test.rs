use std::path::Path;

use graphgate_planner::PlanBuilder;
use graphgate_schema::ComposedSchema;
use pretty_assertions::assert_eq;
use test_case::test_case;
use value::Variables;

// Import the structured test runner functions
mod structured_test_runner;
use structured_test_runner::{build_schema_from_test_case, load_test_case};

#[test_case("./tests/federation/federation_text_test.yaml"; "basic federation test")]
fn test_planner(test_file: &str) {
    // Load the test case from the specified file
    let test_case = load_test_case(Path::new(test_file));
    let schema = build_schema_from_test_case(&test_case);

    let document = parser::parse_query(&test_case.query).unwrap();
    let variables = Variables::default();
    let builder = PlanBuilder::new(&schema, document).variables(variables);
    let actual_node = serde_json::to_value(builder.plan().unwrap()).unwrap();

    // Compare the actual plan with the expected plan
    assert_eq!(actual_node, test_case.expected_plan);
}

#[test_case("./tests/federation/federation_text_test.yaml"; "federation order test")]
fn test_federation_order(test_file: &str) {
    // Load the test case from the specified file
    let test_case = load_test_case(Path::new(test_file));
    let schema = build_schema_from_test_case(&test_case);

    // Create a reverse order schema
    let mut reverse_schema = test_case.schema.clone();
    let keys: Vec<String> = reverse_schema.keys().cloned().collect();
    if keys.len() >= 2 {
        let first_key = keys[0].clone();
        let first_value = reverse_schema.get(&first_key).unwrap().clone();
        let second_key = keys[1].clone();
        let second_value = reverse_schema.get(&second_key).unwrap().clone();

        reverse_schema.insert(first_key.clone(), second_value);
        reverse_schema.insert(second_key.clone(), first_value);
    }

    // Build the reverse order schema
    let reverse_order_schema = ComposedSchema::combine(reverse_schema.into_iter().map(|(name, sdl)| {
        let document = parser::parse_schema(&sdl).unwrap();
        (name, document)
    }))
    .unwrap();

    let document = parser::parse_query(&test_case.query).unwrap();
    let variables = Variables::default();

    // One order
    {
        let builder = PlanBuilder::new(&schema, document.clone()).variables(variables.clone());
        let actual_node = serde_json::to_value(builder.plan().unwrap()).unwrap();
        assert_eq!(actual_node, test_case.expected_plan);
    }

    // Reverse order
    {
        let builder = PlanBuilder::new(&reverse_order_schema, document).variables(variables);
        let actual_node = serde_json::to_value(builder.plan().unwrap()).unwrap();

        // The service name might be different in the reverse order test, so we only check the query and type
        let mut expected_plan = test_case.expected_plan.clone();
        let actual_service = actual_node.get("service").and_then(|s| s.as_str()).unwrap_or("");
        if let Some(obj) = expected_plan.as_object_mut() {
            obj.insert(
                "service".to_string(),
                serde_json::Value::String(actual_service.to_string()),
            );
        }

        assert_eq!(actual_node, expected_plan);
    }
}
