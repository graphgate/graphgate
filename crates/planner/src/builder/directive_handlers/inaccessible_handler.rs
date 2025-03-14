use graphgate_schema::{KeyFields, MetaField, MetaType};
use parser::types::Field;

use crate::{
    builder::{context::Context, directive_registry::DirectiveHandlerTrait},
    plan::ResponsePath,
    types::{FetchEntityGroup, SelectionRefSet},
};

/// Handler for the @inaccessible directive
///
/// The @inaccessible directive is primarily handled during schema composition, not during query planning.
/// This handler is a no-op for query planning, as the inaccessible information is already stored in the schema.
pub struct InaccessibleDirectiveHandler;

impl InaccessibleDirectiveHandler {
    /// Create a new inaccessible directive handler
    pub fn new() -> Self {
        Self
    }
}

impl<'a> DirectiveHandlerTrait<'a> for InaccessibleDirectiveHandler {
    fn name(&self) -> &'static str {
        "inaccessible"
    }

    fn handle(
        &mut self,
        _context: &mut Context<'a>,
        _field: &'a Field,
        _field_definition: &'a MetaField,
        _parent_type: &'a MetaType,
        _current_service: &'a str,
        _directive_args: &'a KeyFields,
        _selection_ref_set: &mut SelectionRefSet<'a>,
        _fetch_entity_group: &mut FetchEntityGroup<'a>,
        _path: &mut ResponsePath<'a>,
    ) {
        // The @inaccessible directive is handled during schema composition
        // and field resolution, not during directive processing.
        // This handler is a no-op.
    }
}
