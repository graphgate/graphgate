use graphgate_schema::MetaType;
use parser::{
    types::{Directive, SelectionSet},
    Pos,
    Positioned,
};

use crate::{Visitor, VisitorContext};

/// Validates that fields specified in `@provides` directives actually exist on the referenced types.
///
/// This rule ensures that when a field uses the `@provides` directive, all the fields specified
/// in the `fields` argument actually exist on the referenced entity type.
#[derive(Default)]
pub struct ProvidesDirectiveFields;

impl<'a> Visitor<'a> for ProvidesDirectiveFields {
    fn enter_directive(&mut self, ctx: &mut VisitorContext<'a>, directive: &'a Positioned<Directive>) {
        // Only check @provides directives
        if directive.node.name.node != "provides" {
            return;
        }

        // Get the parent type from the context
        let _parent_type = match ctx.parent_type() {
            Some(ty) => ty,
            None => return,
        };

        // Get the provides directive's fields argument
        let fields_arg = directive.node.arguments.iter().find(|(name, _)| name.node == "fields");
        let fields_value = match fields_arg {
            Some((_, value)) => match &value.node {
                value::Value::String(s) => s,
                _ => {
                    ctx.report_error(
                        vec![directive.pos],
                        "@provides directive's fields argument must be a string".to_string(),
                    );
                    return;
                },
            },
            None => {
                ctx.report_error(
                    vec![directive.pos],
                    "@provides directive requires a fields argument".to_string(),
                );
                return;
            },
        };

        // Parse the fields string into a selection set
        let selection_set = match parser::parse_query(format!("{{{}}}", fields_value)) {
            Ok(document) => match document.operations {
                parser::types::DocumentOperations::Single(op) => op.node.selection_set.node,
                parser::types::DocumentOperations::Multiple(_) => {
                    ctx.report_error(
                        vec![directive.pos],
                        "Invalid fields in @provides directive: multiple operations not allowed".to_string(),
                    );
                    return;
                },
            },
            Err(err) => {
                ctx.report_error(
                    vec![directive.pos],
                    format!("Invalid fields in @provides directive: {}", err),
                );
                return;
            },
        };

        // Get the current field's return type
        // Since we don't have direct access to the field, we'll use the parent type
        // and assume the field's return type is what we need to validate against
        let field_type = match ctx.current_type() {
            Some(ty) => ty,
            None => return,
        };

        // Validate that all fields in the selection set exist on the field's return type
        self.validate_selection_set(ctx, &selection_set, field_type, directive.pos);
    }
}

impl ProvidesDirectiveFields {
    fn validate_selection_set(
        &self,
        ctx: &mut VisitorContext<'_>,
        selection_set: &SelectionSet,
        parent_type: &MetaType,
        directive_pos: Pos,
    ) {
        for selection in &selection_set.items {
            match &selection.node {
                parser::types::Selection::Field(field) => {
                    let field_name = &field.node.name.node;

                    // Skip __typename as it's always available
                    if field_name == "__typename" {
                        continue;
                    }

                    // Check if the field exists on the parent type
                    match parent_type.field_by_name(field_name) {
                        Some(field_def) => {
                            // If the field has a selection set, validate it recursively
                            if !field.node.selection_set.node.items.is_empty() {
                                // Get the field's return type
                                if let Some(field_type) = ctx.schema.get_type(&field_def.ty) {
                                    self.validate_selection_set(
                                        ctx,
                                        &field.node.selection_set.node,
                                        field_type,
                                        directive_pos,
                                    );
                                }
                            }
                        },
                        None => {
                            ctx.report_error(
                                vec![directive_pos],
                                format!(
                                    "Field '{}' specified in @provides directive does not exist on type '{}'",
                                    field_name, parent_type.name
                                ),
                            );
                        },
                    }
                },
                parser::types::Selection::FragmentSpread(fragment_spread) => {
                    // For fragment spreads, we need to check if the fragment exists and validate its fields
                    let fragment_name = &fragment_spread.node.fragment_name.node;

                    // Try to find the fragment in the document
                    if let Some(fragment) = ctx.fragment(fragment_name) {
                        // Check if the fragment type condition is compatible with the parent type
                        if let Some(fragment_type) = ctx.schema.types.get(&fragment.node.type_condition.node.on.node) {
                            if fragment_type.name != parent_type.name &&
                                !parent_type.is_possible_type(&fragment_type.name)
                            {
                                ctx.report_error(
                                    vec![directive_pos],
                                    format!(
                                        "Fragment '{}' cannot be spread here as type '{}' is not a possible type of \
                                         '{}'",
                                        fragment_name, fragment_type.name, parent_type.name
                                    ),
                                );
                            } else {
                                // Validate the fragment's selection set
                                self.validate_selection_set(
                                    ctx,
                                    &fragment.node.selection_set.node,
                                    parent_type,
                                    directive_pos,
                                );
                            }
                        }
                    } else {
                        ctx.report_error(
                            vec![directive_pos],
                            format!("Unknown fragment '{}' in @provides directive", fragment_name),
                        );
                    }
                },
                parser::types::Selection::InlineFragment(inline_fragment) => {
                    // For inline fragments, validate the selection set
                    // If there's a type condition, check if it's compatible with the parent type
                    if let Some(type_condition) = &inline_fragment.node.type_condition {
                        if let Some(fragment_type) = ctx.schema.types.get(&type_condition.node.on.node) {
                            if fragment_type.name != parent_type.name &&
                                !parent_type.is_possible_type(&fragment_type.name)
                            {
                                ctx.report_error(
                                    vec![directive_pos],
                                    format!(
                                        "Inline fragment cannot be spread here as type '{}' is not a possible type of \
                                         '{}'",
                                        fragment_type.name, parent_type.name
                                    ),
                                );
                                continue;
                            }

                            // Validate the inline fragment's selection set
                            self.validate_selection_set(
                                ctx,
                                &inline_fragment.node.selection_set.node,
                                fragment_type,
                                directive_pos,
                            );
                        }
                    } else {
                        // If there's no type condition, just validate against the parent type
                        self.validate_selection_set(
                            ctx,
                            &inline_fragment.node.selection_set.node,
                            parent_type,
                            directive_pos,
                        );
                    }
                },
            }
        }
    }
}
