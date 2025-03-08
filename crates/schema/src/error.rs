use thiserror::Error;

#[derive(Debug, Error)]
pub enum CombineError {
    #[error("Redefining the schema is not allowed.")]
    SchemaIsNotAllowed,

    #[error("Type '{type_name}' definition conflicted.")]
    DefinitionConflicted { type_name: String },

    #[error("Field '{type_name}.{field_name}' definition conflicted.")]
    FieldConflicted { type_name: String, field_name: String },

    #[error(
        "Value type '{type_name}' is already owned by service '{owner_service}' and cannot be redefined in service \
         '{current_service}' without @shareable directive."
    )]
    ValueTypeOwnershipConflicted {
        type_name: String,
        owner_service: String,
        current_service: String,
    },
}
