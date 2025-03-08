use thiserror::Error;

#[derive(Debug, Error)]
pub enum CombineError {
    #[error(
        "Redefining the schema is not allowed in GraphQL Federation v2. Each subgraph should define its own schema."
    )]
    SchemaIsNotAllowed,

    #[error(
        "Type '{type_name}' has conflicting definitions across subgraphs. In Federation v2, types must have \
         compatible definitions or be marked as @shareable."
    )]
    DefinitionConflicted { type_name: String },

    #[error(
        "Type '{type_name}' has different kinds across subgraphs: '{kind1}' vs '{kind2}'. In Federation v2, types \
         must have the same kind across all subgraphs."
    )]
    TypeKindConflicted {
        type_name: String,
        kind1: String,
        kind2: String,
    },

    #[error(
        "Field '{type_name}.{field_name}' has conflicting definitions across subgraphs. In Federation v2, fields must \
         be explicitly marked as @shareable or be part of an entity key to be shared."
    )]
    FieldConflicted { type_name: String, field_name: String },

    #[error(
        "Field '{type_name}.{field_name}' has conflicting types across subgraphs: '{type1}' vs '{type2}'. In \
         Federation v2, fields with different types cannot be shared even with @shareable."
    )]
    FieldTypeConflicted {
        type_name: String,
        field_name: String,
        type1: String,
        type2: String,
    },

    #[error(
        "Value type '{type_name}' is already owned by service '{owner_service}' and cannot be redefined in service \
         '{current_service}' without @shareable directive. In Federation v2, value types must be owned by a single \
         subgraph unless marked as @shareable."
    )]
    ValueTypeOwnershipConflicted {
        type_name: String,
        owner_service: String,
        current_service: String,
    },

    #[error(
        "Field '{type_name}.{field_name}' is referenced with @external in service '{service}' but is not marked as \
         @shareable in its owning service. In Federation v2, fields must be explicitly marked as @shareable to be \
         referenced with @external in other subgraphs."
    )]
    NonShareableFieldReferenced {
        type_name: String,
        field_name: String,
        service: String,
    },

    #[error(
        "Field '{type_name}.{field_name}' has incompatible arguments across services '{service1}' and '{service2}'. \
         In Federation v2, fields with incompatible arguments must be marked as @shareable to be shared across \
         subgraphs."
    )]
    IncompatibleFieldArguments {
        type_name: String,
        field_name: String,
        service1: String,
        service2: String,
    },

    #[error(
        "Field '{type_name}.{field_name}' is missing required argument '{arg_name}' in service '{service}'. In \
         Federation v2, all required arguments must be consistent across subgraphs or the field must be marked as \
         @shareable."
    )]
    MissingRequiredArgument {
        type_name: String,
        field_name: String,
        arg_name: String,
        service: String,
    },

    #[error(
        "Field '{type_name}.{field_name}' has incompatible argument types for '{arg_name}' across services: '{type1}' \
         vs '{type2}'. In Federation v2, argument types must be compatible across subgraphs or the field must be \
         marked as @shareable."
    )]
    IncompatibleArgumentTypes {
        type_name: String,
        field_name: String,
        arg_name: String,
        type1: String,
        type2: String,
    },

    #[error(
        "Required key field '{field_name}' is missing in entity '{type_name}' referenced in service '{service}'. In \
         Federation v2, all fields referenced in @key directives must be defined in the entity type."
    )]
    KeyFieldsMissing {
        type_name: String,
        field_name: String,
        service: String,
    },
}
