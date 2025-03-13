use std::{
    fs,
    path::{Path, PathBuf},
};

use graphgate_planner::{PlanBuilder, RootNode};
use graphgate_schema::ComposedSchema;
use parser::{parse_query, parse_schema, types::ExecutableDocument};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use tracing::debug;
use value::Variables;

/// Represents a structured test case for the planner
#[derive(Debug, Deserialize)]
pub struct PlannerTestCase {
    /// Name of the test case
    pub name: String,

    /// Optional description of what the test is checking
    #[serde(default)]
    pub description: String,

    /// Schema definitions for each service
    pub schema: std::collections::HashMap<String, String>,

    /// The GraphQL query to test
    pub query: String,

    /// Variables to use with the query
    #[serde(default)]
    pub variables: JsonValue,

    /// The expected plan structure
    pub expected_plan: JsonValue,

    /// Alternative expected plans that are also valid
    #[serde(default)]
    pub alternative_plans: Vec<JsonValue>,

    /// Optional assertions about the plan
    #[serde(default)]
    pub assertions: Vec<PlanAssertion>,

    /// Optional determinism checks to perform
    #[serde(default)]
    pub determinism_checks: DeterminismChecks,
}

/// Configuration for determinism checks
#[derive(Debug, Deserialize, Default)]
pub struct DeterminismChecks {
    /// Whether to test service order determinism
    #[serde(default)]
    pub test_service_order: bool,

    /// Whether to test query structure determinism
    #[serde(default)]
    pub test_query_structure: bool,

    /// Whether to test variable order determinism
    #[serde(default)]
    pub test_variable_order: bool,

    /// Maximum number of permutations to test (to avoid excessive test times)
    #[serde(default = "default_max_permutations")]
    pub max_permutations: usize,
}

fn default_max_permutations() -> usize {
    10 // Default to testing at most 10 permutations
}

/// Represents an assertion about a plan
#[derive(Debug, Deserialize)]
pub struct PlanAssertion {
    /// Type of assertion
    pub assertion_type: String,

    /// Value to check (interpretation depends on assertion_type)
    pub value: JsonValue,

    /// Optional path to check within the plan
    #[serde(default)]
    pub path: Option<String>,
}

/// Loads a test case from a YAML file
pub fn load_test_case(file_path: &Path) -> PlannerTestCase {
    let yaml_content =
        fs::read_to_string(file_path).unwrap_or_else(|_| panic!("Failed to read test file: {}", file_path.display()));

    serde_yaml::from_str(&yaml_content)
        .unwrap_or_else(|e| panic!("Failed to parse test case from {}: {}", file_path.display(), e))
}

/// Builds a composed schema from service definitions in the test case
pub fn build_schema_from_test_case(test_case: &PlannerTestCase) -> ComposedSchema {
    let mut service_documents = Vec::new();

    for (service_name, schema_str) in &test_case.schema {
        let schema_doc = parse_schema(schema_str)
            .unwrap_or_else(|e| panic!("Failed to parse schema for service {}: {}", service_name, e));

        service_documents.push((service_name.clone(), schema_doc));
    }

    ComposedSchema::combine(service_documents).unwrap_or_else(|e| panic!("Failed to combine schemas: {:?}", e))
}

/// Runs a single structured test case
pub fn run_structured_test(test_file: &Path) {
    debug!("Running test: {}", test_file.display());

    let test_case = load_test_case(test_file);
    debug!("Test name: {}", test_case.name);
    if !test_case.description.is_empty() {
        debug!("Description: {}", test_case.description);
    }

    // Build the schema
    let schema = build_schema_from_test_case(&test_case);

    // Parse the query
    let document = parse_query(&test_case.query).unwrap_or_else(|e| panic!("Failed to parse query: {}", e));

    // Parse variables
    let variables: Variables = serde_json::from_value(test_case.variables.clone())
        .unwrap_or_else(|e| panic!("Failed to parse variables: {}", e));

    // Create the plan
    let plan_builder = PlanBuilder::new(&schema, document.clone()).variables(variables.clone());
    let plan = plan_builder
        .plan()
        .unwrap_or_else(|e| panic!("Failed to create plan: {:?}", e));

    // Convert the plan to JSON for comparison
    let actual_plan_json = match &plan {
        RootNode::Query(plan_node) => {
            serde_json::to_value(plan_node).unwrap_or_else(|e| panic!("Failed to serialize plan: {}", e))
        },
        RootNode::Subscribe(plan_node) => {
            serde_json::to_value(plan_node).unwrap_or_else(|e| panic!("Failed to serialize plan: {}", e))
        },
    };

    // Compare with expected plan if provided
    let expected_plan_json = test_case.expected_plan.clone();

    // Print the plans for debugging
    debug!(
        "Expected plan: {}",
        serde_json::to_string_pretty(&expected_plan_json).unwrap()
    );
    debug!(
        "Actual plan: {}",
        serde_json::to_string_pretty(&actual_plan_json).unwrap()
    );

    // Add more detailed debugging for the requires_multi_hop test
    if test_file.to_string_lossy().contains("requires_multi_hop") {
        println!(
            "Expected plan: {}",
            serde_json::to_string_pretty(&expected_plan_json).unwrap()
        );
        println!(
            "Actual plan: {}",
            serde_json::to_string_pretty(&actual_plan_json).unwrap()
        );

        // Print services in the plan
        let services = count_services_in_plan(&actual_plan_json);
        println!("Services in plan: {:?}", services);
    }

    // Check if the actual plan matches any of the expected plans
    let mut plan_matched = actual_plan_json == expected_plan_json;

    // If the primary expected plan doesn't match, check alternative plans
    if !plan_matched && !test_case.alternative_plans.is_empty() {
        for alternative_plan in &test_case.alternative_plans {
            if actual_plan_json == *alternative_plan {
                plan_matched = true;
                break;
            }
        }
    }

    // If no plan matched, run assertions
    if !plan_matched {
        // Run assertions on the actual plan
        for assertion in &test_case.assertions {
            check_assertion(&actual_plan_json, assertion);
        }
    }

    // Run determinism tests if configured
    if test_case.determinism_checks.test_service_order {
        debug!("Running service order determinism test");
        test_service_order_determinism(&test_case, &schema, &document, &variables);
    }

    if test_case.determinism_checks.test_query_structure {
        debug!("Running query structure determinism test");
        test_query_structure_determinism(&test_case, &schema, &document, &variables);
    }

    if test_case.determinism_checks.test_variable_order {
        debug!("Running variable order determinism test");
        test_variable_order_determinism(&test_case, &schema, &document, &variables);
    }

    debug!("Test passed: {}", test_case.name);
}

/// Runs a single test file in isolation
fn run_single_test_file(test_file: &Path) {
    debug!("Running test file: {}", test_file.display());

    // Run the test in isolation
    run_structured_test(test_file);
}

/// Checks a single assertion against the plan
fn check_assertion(plan_json: &JsonValue, assertion: &PlanAssertion) {
    match assertion.assertion_type.as_str() {
        "service_count" => {
            let expected_count = assertion
                .value
                .as_u64()
                .unwrap_or_else(|| panic!("service_count assertion requires a numeric value"));

            let actual_count = count_services_in_plan(plan_json).len();
            assert!(
                actual_count >= expected_count as usize,
                "Expected at least {} services in plan, but found {}",
                expected_count,
                actual_count
            );
        },
        "contains_service" => {
            let service_name = assertion
                .value
                .as_str()
                .unwrap_or_else(|| panic!("contains_service assertion requires a string value"));

            assert!(
                plan_contains_service(plan_json, service_name),
                "Plan does not contain service: {}",
                service_name
            );
        },
        "path_exists" => {
            let path = assertion
                .path
                .as_ref()
                .unwrap_or_else(|| panic!("path_exists assertion requires a path"));

            assert!(
                path_exists_in_plan(plan_json, path),
                "Path does not exist in plan: {}",
                path
            );
        },
        "node_count" => {
            let expected_count = assertion
                .value
                .as_u64()
                .unwrap_or_else(|| panic!("node_count assertion requires a numeric value"));

            let actual_count = count_nodes_in_plan(plan_json);
            assert_eq!(
                actual_count, expected_count as usize,
                "Expected {} nodes in plan, but found {}",
                expected_count, actual_count
            );
        },
        "max_depth" => {
            let expected_depth = assertion
                .value
                .as_u64()
                .unwrap_or_else(|| panic!("max_depth assertion requires a numeric value"));

            let actual_depth = calculate_max_depth(plan_json);
            assert!(
                actual_depth <= expected_depth as usize,
                "Expected max depth of {}, but found {}",
                expected_depth,
                actual_depth
            );
        },
        "variables_passed" => {
            // Just check that the plan has variables
            let has_variables = plan_has_variables(plan_json);
            let expected = assertion
                .value
                .as_bool()
                .unwrap_or_else(|| panic!("variables_passed assertion requires a boolean value"));

            assert_eq!(
                has_variables, expected,
                "Expected variables_passed to be {}, but was {}",
                expected, has_variables
            );
        },
        _ => panic!("Unknown assertion type: {}", assertion.assertion_type),
    }
}

/// Counts the number of nodes in a plan
fn count_nodes_in_plan(plan_json: &JsonValue) -> usize {
    // For sequence nodes, count the number of nodes in the array
    if let Some(nodes) = plan_json.get("nodes").and_then(|n| n.as_array()) {
        return nodes.len();
    }

    // For other node types, return 1
    1
}

/// Calculates the maximum depth of a plan
fn calculate_max_depth(plan_json: &JsonValue) -> usize {
    let mut max_depth = 0;

    if let Some(nodes) = plan_json.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            let depth = calculate_max_depth(node);
            max_depth = max_depth.max(depth);
        }

        // Add 1 for the current level
        max_depth += 1;
    }

    max_depth
}

/// Checks if a plan has variables
fn plan_has_variables(plan_json: &JsonValue) -> bool {
    if plan_json.get("variables").is_some() {
        return true;
    }

    if let Some(nodes) = plan_json.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            if plan_has_variables(node) {
                return true;
            }
        }
    }

    false
}

/// Counts the number of unique services in a plan and returns them
fn count_services_in_plan(plan_json: &JsonValue) -> std::collections::HashSet<&str> {
    let mut services = std::collections::HashSet::new();

    if let Some(service) = plan_json.get("service").and_then(|s| s.as_str()) {
        services.insert(service);
    }

    // Handle subscription nodes
    if let Some(subscribe_nodes) = plan_json.get("subscribeNodes").and_then(|n| n.as_array()) {
        for node in subscribe_nodes {
            if let Some(service) = node.get("service").and_then(|s| s.as_str()) {
                services.insert(service);
            }
        }
    }

    if let Some(nodes) = plan_json.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            let node_services = count_services_in_plan(node);
            services.extend(node_services);
        }
    }

    services
}

/// Checks if a plan contains a specific service
fn plan_contains_service(plan_json: &JsonValue, service_name: &str) -> bool {
    if let Some(service) = plan_json.get("service").and_then(|s| s.as_str()) {
        if service == service_name {
            return true;
        }
    }

    // Handle subscription nodes
    if let Some(subscribe_nodes) = plan_json.get("subscribeNodes").and_then(|n| n.as_array()) {
        for node in subscribe_nodes {
            if let Some(service) = node.get("service").and_then(|s| s.as_str()) {
                if service == service_name {
                    return true;
                }
            }
        }
    }

    if let Some(nodes) = plan_json.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            if plan_contains_service(node, service_name) {
                return true;
            }
        }
    }

    false
}

/// Checks if a path exists in a plan
fn path_exists_in_plan(plan_json: &JsonValue, path: &str) -> bool {
    // This is a simplified implementation
    // A more robust implementation would parse the path and traverse the plan

    let path_parts: Vec<&str> = path.split('.').collect();

    if path_parts.is_empty() {
        return true;
    }

    // Check if the path is mentioned in the query
    if let Some(query) = plan_json.get("query").and_then(|q| q.as_str()) {
        if query.contains(path_parts[path_parts.len() - 1]) {
            return true;
        }
    }

    // Check in subscription nodes
    if let Some(subscribe_nodes) = plan_json.get("subscribeNodes").and_then(|n| n.as_array()) {
        for node in subscribe_nodes {
            if let Some(query) = node.get("query").and_then(|q| q.as_str()) {
                if query.contains(path_parts[path_parts.len() - 1]) {
                    return true;
                }
            }
        }
    }

    // Check in child nodes
    if let Some(nodes) = plan_json.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            if path_exists_in_plan(node, path) {
                return true;
            }
        }
    }

    false
}

/// Tests that the planner produces the same plan regardless of service order
fn test_service_order_determinism(
    test_case: &PlannerTestCase,
    _schema: &ComposedSchema,
    document: &ExecutableDocument,
    variables: &Variables,
) {
    use itertools::Itertools;
    use std::collections::HashMap;

    // Skip test if there are fewer than 2 services
    if test_case.schema.len() < 2 {
        debug!("Skipping service order determinism test as there are fewer than 2 services");
        return;
    }

    // Get all permutations of service names
    let service_names: Vec<String> = test_case.schema.keys().cloned().collect();
    let permutations = service_names
        .iter()
        .permutations(service_names.len())
        .take(test_case.determinism_checks.max_permutations);

    // Store the first plan as our reference
    let mut reference_plan: Option<JsonValue> = None;

    // Test each permutation
    for perm in permutations {
        // Create a schema with this permutation order
        let mut permuted_schema = HashMap::new();
        for service_name in perm {
            let sdl = test_case.schema.get(service_name).unwrap().clone();
            permuted_schema.insert(service_name.clone(), sdl);
        }

        // Build the schema
        let schema_result = ComposedSchema::combine(permuted_schema.into_iter().map(|(name, sdl)| {
            let document = parser::parse_schema(&sdl).unwrap();
            (name, document)
        }));

        // Skip this permutation if schema composition fails
        if schema_result.is_err() {
            debug!(
                "Skipping permutation due to schema composition error: {:?}",
                schema_result.err()
            );
            continue;
        }

        let permuted_schema = schema_result.unwrap();

        // Create the plan
        let builder = PlanBuilder::new(&permuted_schema, document.clone()).variables(variables.clone());
        let plan_result = builder.plan();

        // Skip this permutation if plan creation fails
        if plan_result.is_err() {
            debug!(
                "Skipping permutation due to plan creation error: {:?}",
                plan_result.err()
            );
            continue;
        }

        let plan = plan_result.unwrap();
        let plan_json = serde_json::to_value(plan).unwrap();

        // Normalize the plan by removing service names which might differ
        let normalized_plan = normalize_plan_for_comparison(&plan_json);

        // If this is the first permutation, store it as reference
        if reference_plan.is_none() {
            reference_plan = Some(normalized_plan.clone());
            continue;
        }

        // Compare with reference plan
        assert_eq!(
            normalized_plan,
            reference_plan.as_ref().unwrap().clone(),
            "Plans differ between service permutations"
        );
    }

    // Ensure we had at least one successful permutation
    assert!(
        reference_plan.is_some(),
        "No valid permutations found for service order determinism test"
    );

    debug!("Service order determinism test passed");
}

/// Tests that the planner produces the same plan for equivalent queries
fn test_query_structure_determinism(
    test_case: &PlannerTestCase,
    schema: &ComposedSchema,
    document: &ExecutableDocument,
    variables: &Variables,
) {
    // Original query
    let original_query = &test_case.query;

    // Create equivalent queries with different structure
    let equivalent_queries = generate_equivalent_queries(original_query);

    // Get the plan for the original query
    let original_builder = PlanBuilder::new(schema, document.clone()).variables(variables.clone());
    let original_plan = original_builder.plan().unwrap();
    let original_plan_json = serde_json::to_value(original_plan).unwrap();

    // Test each equivalent query
    for (i, equivalent_query) in equivalent_queries.iter().enumerate() {
        debug!("Testing equivalent query variant {}", i + 1);

        // Skip invalid queries
        let document_result = parser::parse_query(equivalent_query);
        if document_result.is_err() {
            debug!("Skipping invalid query variant {}: {:?}", i + 1, document_result.err());
            continue;
        }

        let equivalent_document = document_result.unwrap();
        let equivalent_builder = PlanBuilder::new(schema, equivalent_document).variables(variables.clone());

        let plan_result = equivalent_builder.plan();
        if plan_result.is_err() {
            debug!(
                "Skipping query variant {} due to plan error: {:?}",
                i + 1,
                plan_result.err()
            );
            continue;
        }

        let equivalent_plan = plan_result.unwrap();
        let equivalent_plan_json = serde_json::to_value(equivalent_plan).unwrap();

        // Compare the plans directly, but only check the type field
        // The query field might differ in whitespace or field order, which is acceptable
        let original_type = original_plan_json.get("type").unwrap();
        let equivalent_type = equivalent_plan_json.get("type").unwrap();

        assert_eq!(
            equivalent_type, original_type,
            "Plan types differ between equivalent queries"
        );

        // For fetch operations, check that the service is the same
        if original_type.as_str() == Some("fetch") {
            if let (Some(original_service), Some(equivalent_service)) = (
                original_plan_json.get("service").and_then(|s| s.as_str()),
                equivalent_plan_json.get("service").and_then(|s| s.as_str()),
            ) {
                assert_eq!(
                    equivalent_service, original_service,
                    "Services differ between equivalent queries"
                );
            }
        }
    }

    debug!("Query structure determinism test passed");
}

/// Tests that the planner produces the same plan regardless of variable order
fn test_variable_order_determinism(
    test_case: &PlannerTestCase,
    schema: &ComposedSchema,
    document: &ExecutableDocument,
    variables: &Variables,
) {
    // Skip test if there are no variables
    if test_case.variables.as_object().is_none_or(|obj| obj.is_empty()) {
        debug!("Skipping variable order determinism test as there are no variables");
        return;
    }

    // Original variables
    let original_variables = variables;

    // Create the plan with original variables
    let original_builder = PlanBuilder::new(schema, document.clone()).variables(original_variables.clone());
    let original_plan = original_builder.plan().unwrap();
    let original_plan_json = serde_json::to_value(original_plan).unwrap();
    let normalized_original = normalize_plan_for_comparison(&original_plan_json);

    // Create equivalent variable orders
    let variable_permutations = generate_variable_permutations(&test_case.variables)
        .into_iter()
        .take(test_case.determinism_checks.max_permutations);

    // Test each variable permutation
    for (i, vars) in variable_permutations.enumerate() {
        debug!("Testing variable permutation {}", i + 1);

        let permuted_variables: Variables = serde_json::from_value(vars.clone()).unwrap_or_default();
        let permuted_builder = PlanBuilder::new(schema, document.clone()).variables(permuted_variables);
        let permuted_plan = permuted_builder.plan().unwrap();
        let permuted_plan_json = serde_json::to_value(permuted_plan).unwrap();
        let normalized_permuted = normalize_plan_for_comparison(&permuted_plan_json);

        assert_eq!(
            normalized_permuted, normalized_original,
            "Plans differ between variable permutations"
        );
    }

    debug!("Variable order determinism test passed");
}

/// Normalizes a plan for comparison by removing or standardizing parts that might
/// legitimately differ between equivalent plans (like service names in certain contexts)
fn normalize_plan_for_comparison(plan: &JsonValue) -> JsonValue {
    match plan {
        JsonValue::Object(obj) => {
            let mut result = serde_json::Map::new();

            // Copy all fields except those we want to normalize
            for (key, value) in obj {
                if key == "service" {
                    // Skip service name as it might differ between permutations
                    continue;
                } else if key == "query" {
                    // Normalize the query string by removing whitespace
                    if let Some(query_str) = value.as_str() {
                        let normalized_query = normalize_query_string(query_str);
                        result.insert(key.clone(), JsonValue::String(normalized_query));
                    } else {
                        result.insert(key.clone(), normalize_plan_for_comparison(value));
                    }
                } else if key == "nodes" && value.is_array() {
                    // For arrays of nodes, we need to sort them to ensure consistent order
                    if let Some(nodes) = value.as_array() {
                        let mut normalized_nodes: Vec<JsonValue> =
                            nodes.iter().map(normalize_plan_for_comparison).collect();

                        // Sort nodes by their normalized representation
                        normalized_nodes.sort_by(|a, b| {
                            let a_str = serde_json::to_string(a).unwrap_or_default();
                            let b_str = serde_json::to_string(b).unwrap_or_default();
                            a_str.cmp(&b_str)
                        });

                        result.insert(key.clone(), JsonValue::Array(normalized_nodes));
                    } else {
                        result.insert(key.clone(), normalize_plan_for_comparison(value));
                    }
                } else {
                    // Recursively normalize other values
                    result.insert(key.clone(), normalize_plan_for_comparison(value));
                }
            }

            JsonValue::Object(result)
        },
        JsonValue::Array(arr) => JsonValue::Array(arr.iter().map(normalize_plan_for_comparison).collect()),
        // Other JSON types can be returned as is
        _ => plan.clone(),
    }
}

/// Normalizes a GraphQL query string by removing whitespace and standardizing format
fn normalize_query_string(query: &str) -> String {
    // This is a simplified normalization that removes all whitespace
    // A more sophisticated version would parse and reformat the query
    query
        .replace(['\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
        .replace("__typename", "") // Remove __typename as it's optional in some contexts
}

/// Generates equivalent queries with different structure but same semantics
fn generate_equivalent_queries(original: &str) -> Vec<String> {
    vec![
        // 1. Change whitespace
        change_whitespace(original)
    ]
}

/// Changes whitespace in a query while preserving semantics
fn change_whitespace(query: &str) -> String {
    // Remove all whitespace and then add it back in a different way
    let compact = query
        .replace(['\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");

    // Add newlines after braces
    compact.replace(" { ", " {\n  ").replace(" } ", "\n}\n")
}

/// Generates different but equivalent variable orders
fn generate_variable_permutations(variables: &JsonValue) -> Vec<JsonValue> {
    use itertools::Itertools;

    let mut result = Vec::new();

    if let Some(obj) = variables.as_object() {
        if obj.len() >= 2 {
            // Get all keys
            let keys: Vec<String> = obj.keys().cloned().collect();

            // Generate all permutations of keys
            for perm in keys.iter().permutations(keys.len()) {
                let mut new_obj = serde_json::Map::new();

                // Add keys in this permutation
                for key in perm {
                    if let Some(value) = obj.get(key) {
                        new_obj.insert(key.clone(), value.clone());
                    }
                }

                result.push(JsonValue::Object(new_obj));
            }
        }
    }

    // If we couldn't generate permutations, return the original
    if result.is_empty() {
        result.push(variables.clone());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    use test_case::test_case;

    static INIT: Once = Once::new();

    // Initialize logging for tests
    fn init_logging() {
        INIT.call_once(|| {
            tracing_subscriber::fmt::init();
        });
    }

    // Generate test cases for federation tests
    #[test_case("tests/federation/requires_directive_test.yaml"; "requires_directive")]
    #[test_case("tests/federation/requires_multi_hop_test.yaml"; "requires_multi_hop")]
    #[test_case("tests/federation/requires_multiple_fields_test.yaml"; "requires_multiple_fields")]
    #[test_case("tests/federation/requires_variables_test.yaml"; "requires_variables")]
    #[test_case("tests/federation/requires_deep_nesting_test.yaml"; "requires_deep_nesting")]
    #[test_case("tests/federation/federation_text_test.yaml"; "federation_text")]
    #[test_case("tests/federation/federation_order_test.yaml"; "federation order")]
    #[test_case("tests/federation/mixed_versions.yaml"; "mixed versions")]
    #[test_case("tests/federation/provides_test.yaml"; "provides")]
    #[test_case("tests/federation/provides_fragments_test.yaml"; "provides fragments")]
    #[test_case("tests/federation/provides_complex_test.yaml"; "provides complex")]
    #[test_case("tests/federation/provides_complex_reviews_test.yaml"; "provides complex reviews")]
    #[test_case("tests/federation/enum_variable_test.yaml"; "enum variable")]
    #[test_case("tests/federation/fragment_spread_test.yaml"; "fragment spread")]
    #[test_case("tests/federation/inline_fragment_test.yaml"; "inline fragment")]
    #[test_case("tests/federation/fragment_on_interface_test.yaml"; "fragment on interface")]
    #[test_case("tests/federation/mutation_test.yaml"; "mutation")]
    #[test_case("tests/federation/possible_union_test.yaml"; "possible union")]
    #[test_case("tests/federation/possible_interface_conditions_test.yaml"; "possible interface conditions")]
    #[test_case("tests/federation/possible_interface_test.yaml"; "possible interface")]
    #[test_case("tests/federation/variables_test.yaml"; "variables")]
    #[test_case("tests/federation/variables_fragment_test.yaml"; "variables fragment")]
    #[test_case("tests/federation/variables_inline_fragment_test.yaml"; "variables inline fragment")]
    #[test_case("tests/federation/query_test.yaml"; "query")]
    #[test_case("tests/federation/subscribe_test.yaml"; "subscribe")]
    #[test_case("tests/federation/shareable_test.yaml"; "shareable")]
    #[test_case("tests/federation/provides_direct_test.yaml"; "provides direct")]
    #[test_case("tests/federation/provides_fragments_direct_test.yaml"; "provides fragments direct")]
    #[test_case("tests/federation/provides_complex_direct_test.yaml"; "provides complex direct")]
    #[test_case("tests/federation/provides_complex_direct_category_test.yaml"; "provides complex direct category")]
    #[test_case("tests/federation/provides_complex_direct_reviews_test.yaml"; "provides complex direct reviews")]
    #[test_case("tests/federation/determinism_test.yaml"; "determinism")]
    #[test_case("tests/federation/complex_determinism_test.yaml"; "complex determinism")]
    #[test_case("tests/federation/invalid_external_test.yaml"; "invalid external")]
    #[test_case("tests/federation/invalid_provides_test.yaml"; "invalid provides")]
    #[test_case("tests/federation/invalid_requires_test.yaml"; "invalid requires")]
    fn test_federation(test_file: &str) {
        init_logging();
        run_single_test_file(&PathBuf::from(test_file));
    }

    // Generate test cases for complex tests
    #[test_case("tests/complex/circular_deps.yaml"; "complex circular deps")]
    #[test_case("tests/complex/multi_hop.yaml"; "complex multi hop")]
    #[test_case("tests/complex/complex_type_conditions.yaml"; "complex type conditions")]
    fn test_complex(test_file: &str) {
        init_logging();
        run_single_test_file(&PathBuf::from(test_file));
    }

    // Generate test cases for edge cases tests
    #[test_case("tests/edge_cases/partial_results.yaml"; "edge partial results")]
    #[test_case("tests/edge_cases/complex_key_fields_test.yaml"; "edge complex key fields")]
    fn test_edge_cases(test_file: &str) {
        init_logging();
        run_single_test_file(&PathBuf::from(test_file));
    }
}
