use graphgate_schema::ComposedSchema;
use parser::types::Type;
use pretty_assertions::assert_eq;

#[test]
fn test_combine_federated_schemas_should_succeed() {
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

#[test]
fn test_combine_federated_schemas_in_any_order_should_return_same_result() {
    let collections_service_document = parser::parse_schema(include_str!("collections.graphql")).unwrap();
    let collectibles_service_document = parser::parse_schema(include_str!("collectibles.graphql")).unwrap();
    let schema_in_asc_order = ComposedSchema::combine([
        ("Collections".to_string(), collections_service_document.clone()),
        ("Collectibles".to_string(), collectibles_service_document.clone()),
    ])
    .unwrap();
    let schema_in_desc_order = ComposedSchema::combine([
        ("Collectibles".to_string(), collectibles_service_document),
        ("Collections".to_string(), collections_service_document),
    ])
    .unwrap();
    let collection_in_asc_order = schema_in_asc_order.get_type(&Type::new("Collection").unwrap());
    let collection_in_desc_order = schema_in_desc_order.get_type(&Type::new("Collection").unwrap());
    assert_eq!(collection_in_asc_order, collection_in_desc_order);
}
