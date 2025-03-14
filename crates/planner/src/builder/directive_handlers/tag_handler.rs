use graphgate_schema::{KeyFields, MetaField, MetaType};
use parser::types::Field;

use crate::{
    builder::{context::Context, directive_registry::DirectiveHandlerTrait, utils::is_list},
    plan::{PathSegment, ResponsePath},
    types::{FetchEntityGroup, FieldRef, SelectionRef, SelectionRefSet},
};

/// Handler for the @tag directive
///
/// The @tag directive is primarily handled during schema composition, not during query planning.
/// This handler is a no-op for query planning, as the tag information is already stored in the schema.
pub struct TagDirectiveHandler;

impl TagDirectiveHandler {
    /// Create a new tag directive handler
    pub fn new() -> Self {
        Self
    }

    /// Process a field with the @tag directive
    ///
    /// This is a no-op for query planning, as the tag information is already stored in the schema.
    /// We simply add the field to the selection set and continue with normal field processing.
    fn process_tag_field<'a>(
        &mut self,
        _context: &mut Context<'a>,
        field: &'a Field,
        field_definition: &'a MetaField,
        _parent_type: &'a MetaType,
        _current_service: &'a str,
        _tag_name: &'a KeyFields,
        selection_ref_set: &mut SelectionRefSet<'a>,
        _fetch_entity_group: &mut FetchEntityGroup<'a>,
        path: &mut ResponsePath<'a>,
    ) {
        // Log the tags for debugging
        if !field_definition.tags.is_empty() {
            tracing::debug!(
                "Processing field with @tag directive: {} (tags: {:?})",
                field.name.node,
                field_definition.tags
            );
        }

        // Add the field to the path for proper tracking
        path.push(PathSegment {
            name: field.response_key().node.as_str(),
            is_list: is_list(&field_definition.ty),
            possible_type: None,
        });

        // Create a selection set for this field
        let sub_selection_set = SelectionRefSet::default();

        // Add the field to the selection set
        selection_ref_set.0.push(SelectionRef::FieldRef(FieldRef {
            field,
            selection_set: sub_selection_set,
        }));

        // Clean up
        path.pop();
    }
}

impl<'a> DirectiveHandlerTrait<'a> for TagDirectiveHandler {
    fn name(&self) -> &'static str {
        "tag"
    }

    fn handle(
        &mut self,
        context: &mut Context<'a>,
        field: &'a Field,
        field_definition: &'a MetaField,
        parent_type: &'a MetaType,
        current_service: &'a str,
        tag_name: &'a KeyFields,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        path: &mut ResponsePath<'a>,
    ) {
        // Process the field with the @tag directive
        self.process_tag_field(
            context,
            field,
            field_definition,
            parent_type,
            current_service,
            tag_name,
            selection_ref_set,
            fetch_entity_group,
            path,
        );
    }
}
