use std::fs;

use graphgate_planner::{PlanBuilder, RootNode};
use graphgate_schema::ComposedSchema;
use parser::parse_schema;

#[test]
fn test_provides_directive_with_nested_fields() {
    // Load and parse the test schema
    let schema_str = fs::read_to_string("tests/provides_complex_test.graphql").expect("Failed to read schema file");
    let schema_doc = parse_schema(&schema_str).expect("Failed to parse schema");
    let schema = ComposedSchema::new(schema_doc);

    // Test case 1: Query that should use the @provides directive for reviews and author
    let query1 = r#"
    {
      products {
        id
        name
        reviews {
          id
          author {
            id
            name
            profile {
              bio
              avatarUrl
            }
          }
        }
      }
    }
    "#;

    // Parse the query and create a plan
    let document1 = parser::parse_query(query1).expect("Failed to parse query");
    let plan_builder1 = PlanBuilder::new(&schema, document1);
    let plan1 = plan_builder1.plan().expect("Failed to create plan");

    // Verify the plan
    match plan1 {
        RootNode::Query(plan_node) => {
            // Convert the plan to a string for easier inspection
            let plan_json = serde_json::to_string_pretty(&plan_node).expect("Failed to serialize plan");
            
            // The plan should not include a fetch to the "users" service
            // because the "reviews" field on Product provides the author.id, name, and profile
            assert!(!plan_json.contains(r#""service":"users""#), 
                   "Plan should not include a fetch to the users service");
            
            // Print the plan for debugging
            println!("Plan 1: {}", plan_json);
        }
        _ => panic!("Expected a query plan"),
    }

    // Test case 2: Query that should use the @provides directive for category and parent
    let query2 = r#"
    {
      products {
        id
        name
        category {
          id
          name
          parent {
            id
            name
          }
        }
      }
    }
    "#;

    // Parse the query and create a plan
    let document2 = parser::parse_query(query2).expect("Failed to parse query");
    let plan_builder2 = PlanBuilder::new(&schema, document2);
    let plan2 = plan_builder2.plan().expect("Failed to create plan");

    // Verify the plan
    match plan2 {
        RootNode::Query(plan_node) => {
            // Convert the plan to a string for easier inspection
            let plan_json = serde_json::to_string_pretty(&plan_node).expect("Failed to serialize plan");
            
            // The plan should not include a fetch to the "categories" service
            // because the "category" field on Product provides the category.id, name, and parent
            assert!(!plan_json.contains(r#""service":"categories""#), 
                   "Plan should not include a fetch to the categories service");
            
            // Print the plan for debugging
            println!("Plan 2: {}", plan_json);
        }
        _ => panic!("Expected a query plan"),
    }

    // Test case 3: Query that should NOT use the @provides directive because it requests fields not provided
    let query3 = r#"
    {
      reviews {
        id
        text
        rating
        author {
          id
          name
          email
        }
      }
    }
    "#;

    // Parse the query and create a plan
    let document3 = parser::parse_query(query3).expect("Failed to parse query");
    let plan_builder3 = PlanBuilder::new(&schema, document3);
    let plan3 = plan_builder3.plan().expect("Failed to create plan");

    // Verify the plan
    match plan3 {
        RootNode::Query(plan_node) => {
            // Convert the plan to a string for easier inspection
            let plan_json = serde_json::to_string_pretty(&plan_node).expect("Failed to serialize plan");
            
            // Print the plan for debugging
            println!("Plan 3: {}", plan_json);
            
            // The plan SHOULD include a fetch to the "reviews" service
            // because we're directly querying the reviews service
            let contains_reviews_query = plan_json.contains(r#"reviews { id text rating"#);
            assert!(contains_reviews_query, "Plan should include a query for reviews");
        }
        _ => panic!("Expected a query plan"),
    }
} 