use parser::{
    types::{BaseType, DocumentOperations, ExecutableDocument, OperationDefinition, Type},
    Positioned,
};
use tracing::instrument;

/// Maximum path length to prevent infinite recursion
pub const MAX_PATH_LENGTH: usize = 20;

/// Maximum number of times a field can appear in a path
pub const MAX_FIELD_REPETITIONS: usize = 2;

/// Minimum path length to start checking for patterns
pub const MIN_PATH_LENGTH_FOR_PATTERN_CHECK: usize = 4;

/// Check if a type is a list type
#[inline]
pub fn is_list(ty: &Type) -> bool {
    matches!(ty.base, BaseType::List(_))
}

/// Get the operation definition from a document
#[instrument(ret, level = "trace")]
pub fn get_operation<'a>(
    document: &'a ExecutableDocument,
    operation_name: Option<&str>,
) -> &'a Positioned<OperationDefinition> {
    let operation = if let Some(operation_name) = operation_name {
        match &document.operations {
            DocumentOperations::Single(_) => None,
            DocumentOperations::Multiple(operations) => operations.get(operation_name),
        }
    } else {
        match &document.operations {
            DocumentOperations::Single(operation) => Some(operation),
            DocumentOperations::Multiple(map) if map.len() == 1 => Some(map.iter().next().unwrap().1),
            DocumentOperations::Multiple(_) => None,
        }
    };
    operation.expect("The query validator should find this error.")
}
