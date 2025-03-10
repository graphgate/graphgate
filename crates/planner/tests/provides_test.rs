use std::fs;

use graphgate_planner::{PlanBuilder, RootNode};
use graphgate_schema::ComposedSchema;
use parser::parse_schema;

#[test]
fn test_provides_directive() {
    // Load and parse the test schema
    let schema_str = fs::read_to_string("tests/provides_test.graphql").expect("Failed to read schema file");
    let schema_doc = parse_schema(&schema_str).expect("Failed to parse schema");
    let schema = ComposedSchema::new(schema_doc);

    // Create a query that should use the @provides directive
    let query = r#"
    {
      products {
        id
        name
        reviews {
          id
          author {
            id
            name
          }
        }
      }
    }
    "#;

    // Parse the query and create a plan
    let document = parser::parse_query(query).expect("Failed to parse query");
    let plan_builder = PlanBuilder::new(&schema, document);
    let plan = plan_builder.plan().expect("Failed to create plan");

    // Verify the plan
    match plan {
        RootNode::Query(plan_node) => {
            // Convert the plan to a string for easier inspection
            let plan_json = serde_json::to_string_pretty(&plan_node).expect("Failed to serialize plan");
            
            // The plan should not include a fetch to the "users" service
            // because the "reviews" field on Product provides the author.id and author.name
            assert!(!plan_json.contains(r#""service":"users""#), 
                   "Plan should not include a fetch to the users service");
            
            // Print the plan for debugging
            println!("Plan: {}", plan_json);
        }
        _ => panic!("Expected a query plan"),
    }
} 