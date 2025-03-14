use graphgate_schema::{KeyFields, MetaField, MetaType};
use parser::types::Field;
use std::collections::HashSet;

use crate::{
    builder::{context::Context, directive_registry::DirectiveHandlerTrait, utils::is_list},
    plan::{PathSegment, ResponsePath},
    types::{FetchEntity, FetchEntityGroup, FetchEntityKey, FieldRef, SelectionRef, SelectionRefSet},
};

/// Handler for the @provides directive
pub struct ProvidesDirectiveHandler;

impl ProvidesDirectiveHandler {
    /// Create a new provides directive handler
    pub fn new() -> Self {
        Self
    }

    /// Check if the @provides directive can satisfy the requested fields
    fn can_satisfy_selection_set<'a>(
        &self,
        context: &Context<'a>,
        field_name: &str,
        selection_set: &parser::types::SelectionSet,
        provides: &'a KeyFields,
    ) -> bool {
        context.selection_set_satisfied_by_provides(field_name, selection_set, provides)
    }

    /// Add a fetch entity to the group
    #[allow(clippy::too_many_arguments)]
    fn add_fetch_entity<'a>(
        &mut self,
        context: &mut Context<'a>,
        field: &'a Field,
        _field_definition: &'a MetaField,
        parent_type: &'a MetaType,
        service: &'a str,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        path: &mut ResponsePath<'a>,
    ) {
        // Generate a unique prefix for the entity
        let prefix = context.take_key_prefix();

        // Create a fetch entity key for the service
        let fetch_entity_key = FetchEntityKey {
            service,
            path: path.clone(),
            parent_type: parent_type.name.as_str(),
        };

        // Add or update the entity in the fetch group
        if !fetch_entity_group.contains_key(&fetch_entity_key) {
            fetch_entity_group.insert(fetch_entity_key.clone(), FetchEntity {
                parent_type,
                prefix,
                fields: vec![field],
            });
        } else if let Some(fetch_entity) = fetch_entity_group.get_mut(&fetch_entity_key) {
            fetch_entity.fields.push(field);
        }

        // Add the field to the selection set
        let sub_selection_set = SelectionRefSet::default();
        selection_ref_set.0.push(SelectionRef::FieldRef(FieldRef {
            field,
            selection_set: sub_selection_set,
        }));
    }
}

impl<'a> DirectiveHandlerTrait<'a> for ProvidesDirectiveHandler {
    fn name(&self) -> &'static str {
        "provides"
    }

    fn handle(
        &mut self,
        context: &mut Context<'a>,
        field: &'a Field,
        field_definition: &'a MetaField,
        parent_type: &'a MetaType,
        current_service: &'a str,
        provides: &'a KeyFields,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        path: &mut ResponsePath<'a>,
    ) {
        // Add the field to the path for proper tracking
        path.push(PathSegment {
            name: field.response_key().node.as_str(),
            is_list: is_list(&field_definition.ty),
            possible_type: None,
        });

        // Check if the @provides directive can satisfy the requested fields
        let can_satisfy =
            self.can_satisfy_selection_set(context, field.name.node.as_str(), &field.selection_set.node, provides);

        if can_satisfy {
            // If the @provides directive can satisfy the requested fields,
            // we can process the field in the current service
            self.add_fetch_entity(
                context,
                field,
                field_definition,
                parent_type,
                current_service,
                selection_ref_set,
                fetch_entity_group,
                path,
            );
        } else {
            // If the @provides directive cannot satisfy the requested fields,
            // we need to fetch the field from the service that owns it

            // Find the type of the field
            let field_type_name = match &field_definition.ty.base {
                parser::types::BaseType::Named(name) => name.as_str(),
                _ => {
                    path.pop();
                    return;
                },
            };

            // Find the service that owns the field's type
            if let Some(field_type) = context.schema.types.get(field_type_name) {
                if let Some(owner) = &field_type.owner {
                    // Add a fetch entity for the field from the owner service
                    self.add_fetch_entity(
                        context,
                        field,
                        field_definition,
                        parent_type,
                        owner,
                        selection_ref_set,
                        fetch_entity_group,
                        path,
                    );
                } else {
                    // If the field type doesn't have an owner, try to find services that have keys for it
                    let mut external_services = HashSet::new();
                    for service_name in field_type.keys.keys() {
                        if service_name != current_service {
                            external_services.insert(service_name.as_str());
                        }
                    }

                    if !external_services.is_empty() {
                        // Use the first external service we found
                        let service = *external_services.iter().next().unwrap();
                        self.add_fetch_entity(
                            context,
                            field,
                            field_definition,
                            parent_type,
                            service,
                            selection_ref_set,
                            fetch_entity_group,
                            path,
                        );
                    } else {
                        // If we can't find any external services, use the current service
                        self.add_fetch_entity(
                            context,
                            field,
                            field_definition,
                            parent_type,
                            current_service,
                            selection_ref_set,
                            fetch_entity_group,
                            path,
                        );
                    }
                }
            } else {
                // If we can't find the field type, use the current service
                self.add_fetch_entity(
                    context,
                    field,
                    field_definition,
                    parent_type,
                    current_service,
                    selection_ref_set,
                    fetch_entity_group,
                    path,
                );
            }
        }

        // Clean up
        path.pop();
    }
}
