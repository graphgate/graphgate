use graphgate_schema::{KeyFields, MetaField, MetaType};
use parser::types::Field;
use std::collections::HashSet;

use super::{
    context::Context,
    utils::{is_list, MAX_PATH_LENGTH},
};
use crate::{
    plan::{PathSegment, ResponsePath},
    types::{FetchEntity, FetchEntityGroup, FetchEntityKey, FieldRef, SelectionRef, SelectionRefSet},
};

/// Handler for GraphQL directives
pub struct DirectiveHandler<'a> {
    context: Option<&'a mut Context<'a>>,
}

impl<'a> DirectiveHandler<'a> {
    /// Create a new directive handler
    pub fn new() -> Self {
        Self { context: None }
    }

    /// Set the context for the directive handler
    pub fn set_context(&mut self, context: &'a mut Context<'a>) {
        self.context = Some(context);
    }

    /// Handle the @requires directive
    #[allow(clippy::too_many_arguments)]
    pub fn handle_requires_directive(
        &mut self,
        field: &'a Field,
        field_definition: &'a MetaField,
        parent_type: &'a MetaType,
        current_service: &'a str,
        requires: &'a KeyFields,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        path: &mut ResponsePath<'a>,
    ) {
        // Safety mechanism to prevent infinite recursion
        if path.len() > MAX_PATH_LENGTH {
            return;
        }

        // Check for field appearing too many times in path (potential loop)
        if self.should_skip_due_to_recursion(path, field) {
            return;
        }

        // Add field to path and selection set
        self.add_field_to_path_and_selection(path, selection_ref_set, field, field_definition);

        // Process entity for current service
        let prefix =
            self.process_entity_for_current_service(fetch_entity_group, current_service, parent_type, field, path);

        // Find required external services
        let external_services = self.find_external_services_for_requires(requires, parent_type, current_service);

        // Create fetch entities for external services
        self.create_fetch_entities_for_external_services(
            fetch_entity_group,
            external_services,
            path,
            parent_type,
            prefix,
        );

        // Clean up
        path.pop();
    }

    /// Check if we should skip processing due to recursion
    fn should_skip_due_to_recursion(&self, path: &ResponsePath<'a>, field: &'a Field) -> bool {
        let mut field_count = 0;
        for segment in path.iter() {
            if segment.name == field.response_key().node.as_str() {
                field_count += 1;
                if field_count >= 3 {
                    return true;
                }
            }
        }
        false
    }

    /// Add field to path and selection set
    fn add_field_to_path_and_selection(
        &self,
        path: &mut ResponsePath<'a>,
        selection_ref_set: &mut SelectionRefSet<'a>,
        field: &'a Field,
        field_definition: &'a MetaField,
    ) {
        // Add the field to the path for proper tracking
        path.push(PathSegment {
            name: field.response_key().node.as_str(),
            is_list: is_list(&field_definition.ty),
            possible_type: None,
        });

        // Add the field to the selection set
        let sub_selection_set = SelectionRefSet::default();
        selection_ref_set.0.push(SelectionRef::FieldRef(FieldRef {
            field,
            selection_set: sub_selection_set,
        }));
    }

    /// Process entity for current service
    fn process_entity_for_current_service(
        &mut self,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        current_service: &'a str,
        parent_type: &'a MetaType,
        field: &'a Field,
        path: &ResponsePath<'a>,
    ) -> usize {
        let context = self.context.as_mut().expect("Context not set");

        // Get the current key ID for this entity
        let prefix = context.take_key_prefix();

        // Create a fetch entity key for the current service
        let fetch_entity_key = FetchEntityKey {
            service: current_service,
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

        prefix
    }

    /// Find external services required by the @requires directive
    fn find_external_services_for_requires(
        &mut self,
        requires: &'a KeyFields,
        parent_type: &'a MetaType,
        current_service: &'a str,
    ) -> HashSet<&'a str> {
        let context = self.context.as_ref().expect("Context not set");
        let mut external_services = HashSet::new();

        // Parse the requires directive to find required field references
        let required_entities = self.parse_requires_directive(requires);

        // For each entity referenced in the @requires directive
        for (entity_name, _) in required_entities {
            // Extract the base name without any arguments
            let entity_base_name = if let Some(idx) = entity_name.find('(') {
                &entity_name[0..idx]
            } else {
                entity_name
            };

            // Look for a field in the parent type with this name
            if let Some(entity_field) = parent_type.fields.get(entity_base_name) {
                // If this field resolves to an entity type and has a reference to another service
                if let Some(entity_type_name) = context.get_named_type(&entity_field.ty) {
                    if let Some(entity_type) = context.schema.types.get(entity_type_name) {
                        // Check for key definitions to find services
                        for service_name in entity_type.keys.keys() {
                            if service_name != current_service {
                                external_services.insert(service_name.as_str());
                            }
                        }

                        // Also check if the entity type has an owner
                        if let Some(owner) = &entity_type.owner {
                            if owner != current_service {
                                external_services.insert(owner.as_str());
                            }
                        }
                    }
                }
            }
        }

        // If we couldn't find any external services, try looking at all services that have keys for this parent type
        if external_services.is_empty() {
            for service_name in parent_type.keys.keys() {
                if service_name != current_service {
                    external_services.insert(service_name.as_str());
                }
            }
        }

        external_services
    }

    /// Create fetch entities for external services
    fn create_fetch_entities_for_external_services(
        &mut self,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        external_services: HashSet<&'a str>,
        path: &ResponsePath<'a>,
        parent_type: &'a MetaType,
        prefix: usize,
    ) {
        // Create fetch entities for all external services
        for service in external_services {
            let required_field_key = FetchEntityKey {
                service,
                path: path.clone(),
                parent_type: parent_type.name.as_str(),
            };

            if !fetch_entity_group.contains_key(&required_field_key) {
                fetch_entity_group.insert(required_field_key, FetchEntity {
                    parent_type,
                    prefix,
                    fields: vec![],
                });
            }
        }
    }

    /// Parse fields from a @requires directive
    pub fn parse_requires_directive(&self, requires: &'a KeyFields) -> Vec<(&'a str, &'a KeyFields)> {
        let mut entities = Vec::new();

        for (field_path, subfields) in requires.iter() {
            // A field path might be something like "user" or "user(id: $id)"
            // or a nested path like "user { country }"
            if field_path.contains('{') {
                // This is a nested path, extract the entity name
                if let Some(idx) = field_path.find('{') {
                    let entity_name = field_path[0..idx].trim();
                    entities.push((entity_name, subfields));
                }
            } else {
                // This is a simple path
                entities.push((field_path.as_str(), subfields));
            }
        }

        entities
    }
}
