use graphgate_schema::{MetaField, MetaType};
use parser::types::Field;

use crate::{
    builder::{
        context::Context,
        directive_handlers::{
            InaccessibleDirectiveHandler,
            ProvidesDirectiveHandler,
            RequiresDirectiveHandler,
            TagDirectiveHandler,
        },
        directive_registry::DirectiveRegistry,
        utils::{is_list, MAX_FIELD_REPETITIONS, MAX_PATH_LENGTH, MIN_PATH_LENGTH_FOR_PATTERN_CHECK},
    },
    plan::{PathSegment, ResponsePath},
    types::{FetchEntity, FetchEntityGroup, FetchEntityKey, FieldRef, SelectionRef, SelectionRefSet},
};

/// Resolver for GraphQL fields
pub struct FieldResolver<'a> {
    pub(super) context: &'a mut Context<'a>,
    directive_registry: DirectiveRegistry<'a>,
}

impl<'a> FieldResolver<'a> {
    /// Create a new field resolver
    pub fn new(context: &'a mut Context<'a>) -> Self {
        // Create a new field resolver
        let mut resolver = Self {
            context,
            directive_registry: DirectiveRegistry::new(),
        };

        // Register directive handlers
        resolver
            .directive_registry
            .register(Box::new(RequiresDirectiveHandler::new()));
        resolver
            .directive_registry
            .register(Box::new(ProvidesDirectiveHandler::new()));
        resolver
            .directive_registry
            .register(Box::new(TagDirectiveHandler::new()));
        resolver
            .directive_registry
            .register(Box::new(InaccessibleDirectiveHandler::new()));

        resolver
    }

    /// Build a field in the query plan
    pub fn build_field(
        &mut self,
        path: &mut ResponsePath<'a>,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        current_service: &'a str,
        parent_type: &'a MetaType,
        field: &'a Field,
    ) {
        // Check for potential infinite recursion - limit the path length
        if path.len() > MAX_PATH_LENGTH {
            return;
        }

        // Check for repeated field patterns in the path that might indicate a loop
        if self.should_skip_due_to_recursion(path, field) {
            return;
        }

        // Get field definition
        let field_definition = match self.get_field_definition(parent_type, field) {
            Some(def) => def,
            None => return,
        };

        // Skip inaccessible fields
        if field_definition.inaccessible {
            tracing::debug!("Skipping inaccessible field: {}", field.name.node);
            return;
        }

        // Skip fields with inaccessible return types
        if let Some(return_type) = self.context.schema.get_type(&field_definition.ty) {
            if return_type.inaccessible {
                tracing::debug!("Skipping field with inaccessible return type: {}", field.name.node);
                return;
            }
        }

        // Determine service
        let service = self.determine_service_for_field(field_definition, parent_type, current_service, field);

        // Handle @tag directive
        if !field_definition.tags.is_empty() {
            // For tagged fields, we'll use the TagDirectiveHandler
            // We need a KeyFields object to pass to the handler, but it's not actually used
            // So we'll just use any available KeyFields from the parent type
            if let Some(keys) = parent_type.keys.get(current_service).and_then(|keys| keys.first()) {
                if let Some(handler) = self.directive_registry.get_mut("tag") {
                    handler.handle(
                        self.context,
                        field,
                        field_definition,
                        parent_type,
                        current_service,
                        keys,
                        selection_ref_set,
                        fetch_entity_group,
                        path,
                    );
                    return;
                }
            }

            // If we couldn't find any KeyFields or the handler, we'll just log and continue
            tracing::debug!("Found field with @tag directive: {}", field.name.node);
        }

        // Handle @provides directive
        if let Some(provides) = &field_definition.provides {
            // Check if the @provides directive can satisfy the requested fields
            let provides_can_satisfy = self.context.selection_set_satisfied_by_provides(
                field.name.node.as_str(),
                &field.selection_set.node,
                provides,
            );

            // Only process @provides if it can satisfy the requested fields
            if provides_can_satisfy {
                // Use the directive registry to handle the provides directive
                if let Some(handler) = self.directive_registry.get_mut("provides") {
                    handler.handle(
                        self.context,
                        field,
                        field_definition,
                        parent_type,
                        current_service,
                        provides,
                        selection_ref_set,
                        fetch_entity_group,
                        path,
                    );
                    return;
                }
            }
        }

        // Handle @requires directive
        if let Some(requires) = &field_definition.requires {
            // Only process @requires if we're in the service that owns the field
            if service == current_service {
                // Use the directive registry to handle the requires directive
                if let Some(handler) = self.directive_registry.get_mut("requires") {
                    handler.handle(
                        self.context,
                        field,
                        field_definition,
                        parent_type,
                        current_service,
                        requires,
                        selection_ref_set,
                        fetch_entity_group,
                        path,
                    );
                    return;
                } else {
                    // If we can't find the requires handler, log a warning and continue with normal processing
                    tracing::warn!(
                        "RequiresDirectiveHandler not found in registry for field: {}",
                        field.name.node
                    );
                }
            }
        }

        // Handle service mismatch
        if service != current_service {
            self.handle_service_mismatch(
                path,
                selection_ref_set,
                fetch_entity_group,
                service,
                current_service,
                parent_type,
                field,
                field_definition,
            );
            return;
        }

        // Process field in current service
        self.process_field_in_current_service(
            path,
            selection_ref_set,
            fetch_entity_group,
            service,
            parent_type,
            field,
            field_definition,
        );
    }

    /// Check if we should skip processing due to recursion
    fn should_skip_due_to_recursion(&self, path: &ResponsePath<'a>, field: &'a Field) -> bool {
        if path.len() > MIN_PATH_LENGTH_FOR_PATTERN_CHECK {
            let mut pattern_count = 0;
            let field_name = field.name.node.as_str();
            for segment in path.iter() {
                if segment.name == field_name {
                    pattern_count += 1;
                    // If we see the same field name more than twice in the path, it's likely a loop
                    if pattern_count > MAX_FIELD_REPETITIONS {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Get the field definition from the parent type
    fn get_field_definition(&self, parent_type: &'a MetaType, field: &Field) -> Option<&'a MetaField> {
        let field_name = field.name.node.as_str();
        parent_type.fields.get(field_name)
    }

    /// Determine the service that owns a field
    fn determine_service_for_field(
        &self,
        field_definition: &'a MetaField,
        parent_type: &'a MetaType,
        current_service: &'a str,
        field: &'a Field,
    ) -> &'a str {
        // Check if the field has a specific service defined
        let field_service = field_definition.service.as_deref();

        // First check if the field can be provided by the current service via @provides
        let can_be_provided =
            self.context
                .can_field_be_provided(parent_type, field, current_service, &field.selection_set.node);

        // Determine the service to use for this field
        // Prefer the current service if it can provide the field to minimize service hops
        // If the field has a specific service defined, use that service
        if let Some(service) = field_service {
            // If the field has a specific service defined, use that service
            service
        } else if can_be_provided {
            current_service
        } else {
            match parent_type.owner.as_deref() {
                Some(service) => service,
                None => current_service,
            }
        }
    }

    /// Handle the case where a field needs to be fetched from a different service
    #[allow(clippy::too_many_arguments)]
    fn handle_service_mismatch(
        &mut self,
        path: &mut ResponsePath<'a>,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        service: &'a str,
        _current_service: &'a str,
        parent_type: &'a MetaType,
        field: &'a Field,
        field_definition: &'a MetaField,
    ) {
        // We need to fetch this field from another service
        path.push(PathSegment {
            name: field.response_key().node.as_str(),
            is_list: is_list(&field_definition.ty),
            possible_type: None,
        });

        let mut keys = parent_type.keys.get(service).and_then(|x| x.first());
        if keys.is_none() {
            if let Some(owner) = &parent_type.owner {
                keys = parent_type.keys.get(owner).and_then(|x| x.first());
            }
        }
        let keys = match keys {
            Some(keys) => keys,
            None => {
                path.pop();
                return;
            },
        };

        // Check if the field is defined in the service
        // If the field is defined in the service but not in the keys, we need to fetch it
        let field_defined_in_service = field_definition.service.as_deref() == Some(service);

        // Check if the field has a @provides directive
        // If the field has a @provides directive, we need to fetch it from the service that defines it
        let field_has_provides = field_definition.provides.is_some();

        // Check if the @provides directive can actually satisfy the requested fields
        let provides_can_satisfy = if let Some(provides) = &field_definition.provides {
            self.context.selection_set_satisfied_by_provides(
                field.name.node.as_str(),
                &field.selection_set.node,
                provides,
            )
        } else {
            false
        };

        // If the field is defined in the service, has a valid @provides directive, or not in the keys, we need to
        // fetch it from the current service
        if field_defined_in_service ||
            (field_has_provides && provides_can_satisfy) ||
            !self.context.field_in_keys(field, keys)
        {
            // Force the field to be fetched from the service that defines it
            self.add_fetch_entity(
                field,
                field_definition,
                parent_type,
                service,
                selection_ref_set,
                fetch_entity_group,
                path,
            );
            return;
        }

        // If the field has an invalid @provides directive (can't satisfy the requested fields),
        // we need to fetch the fields from the service that owns the field's type
        if field_has_provides && !provides_can_satisfy {
            // Find the type of the field
            let field_type_name = match &field_definition.ty.base {
                parser::types::BaseType::Named(name) => name.as_str(),
                _ => {
                    path.pop();
                    return;
                },
            };

            // Find the service that owns the field's type
            if let Some(field_type) = self.context.schema.types.get(field_type_name) {
                if let Some(owner) = &field_type.owner {
                    // Add a fetch entity for the field from the owner service
                    self.add_fetch_entity(
                        field,
                        field_definition,
                        parent_type,
                        owner,
                        selection_ref_set,
                        fetch_entity_group,
                        path,
                    );
                    return;
                }
            }
        }

        path.pop();
    }

    /// Process a field in the current service
    #[allow(clippy::too_many_arguments)]
    fn process_field_in_current_service(
        &mut self,
        path: &mut ResponsePath<'a>,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        service: &'a str,
        _parent_type: &'a MetaType,
        field: &'a Field,
        field_definition: &'a MetaField,
    ) {
        path.push(PathSegment {
            name: field.response_key().node.as_str(),
            is_list: is_list(&field_definition.ty),
            possible_type: None,
        });

        // Create a selection set for this field
        let mut sub_selection_set = SelectionRefSet::default();

        // If the field has a selection set, build it
        if !field.selection_set.node.items.is_empty() {
            if let Some(field_type) = self.context.schema.get_type(&field_definition.ty) {
                if matches!(
                    field_type.kind,
                    graphgate_schema::TypeKind::Interface | graphgate_schema::TypeKind::Union
                ) {
                    // For abstract types (interfaces and unions), we need special handling
                    self.build_abstract_selection_set(
                        path,
                        &mut sub_selection_set,
                        fetch_entity_group,
                        service,
                        field_type,
                        &field.selection_set.node,
                    );
                } else {
                    // For concrete types, we can build the selection set directly
                    self.build_selection_set(
                        path,
                        &mut sub_selection_set,
                        fetch_entity_group,
                        service,
                        field_type,
                        &field.selection_set.node,
                    );
                }
            }
        }

        selection_ref_set.0.push(SelectionRef::FieldRef(FieldRef {
            field,
            selection_set: sub_selection_set,
        }));
        path.pop();
    }

    /// Add a fetch entity to the group
    #[allow(clippy::too_many_arguments)]
    fn add_fetch_entity(
        &mut self,
        field: &'a Field,
        _field_definition: &'a MetaField,
        parent_type: &'a MetaType,
        service: &'a str,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        path: &mut ResponsePath<'a>,
    ) {
        // Generate a unique prefix for the entity
        let prefix = self.context.take_key_prefix();

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

    /// Build a selection set for a concrete type
    pub fn build_selection_set(
        &mut self,
        path: &mut ResponsePath<'a>,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        current_service: &'a str,
        parent_type: &'a MetaType,
        selection_set: &'a parser::types::SelectionSet,
    ) {
        use parser::types::Selection;

        for selection in &selection_set.items {
            match &selection.node {
                Selection::Field(field) => {
                    self.build_field(
                        path,
                        selection_ref_set,
                        fetch_entity_group,
                        current_service,
                        parent_type,
                        &field.node,
                    );
                },
                Selection::FragmentSpread(fragment_spread) => {
                    if let Some(fragment) = self
                        .context
                        .fragments
                        .get(fragment_spread.node.fragment_name.node.as_str())
                    {
                        self.build_selection_set(
                            path,
                            selection_ref_set,
                            fetch_entity_group,
                            current_service,
                            parent_type,
                            &fragment.node.selection_set.node,
                        );
                    }
                },
                Selection::InlineFragment(inline_fragment) => {
                    self.build_selection_set(
                        path,
                        selection_ref_set,
                        fetch_entity_group,
                        current_service,
                        parent_type,
                        &inline_fragment.node.selection_set.node,
                    );
                },
            }
        }
    }

    /// Build a selection set for an abstract type (interface or union)
    pub fn build_abstract_selection_set(
        &mut self,
        path: &mut ResponsePath<'a>,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        current_service: &'a str,
        parent_type: &'a MetaType,
        selection_set: &'a parser::types::SelectionSet,
    ) {
        use indexmap::IndexMap;
        use parser::types::Selection;

        fn build_fields<'a>(
            resolver: &mut FieldResolver<'a>,
            path: &mut ResponsePath<'a>,
            selection_ref_set_group: &mut IndexMap<&'a str, SelectionRefSet<'a>>,
            fetch_entity_group: &mut FetchEntityGroup<'a>,
            current_service: &'a str,
            selection_set: &'a parser::types::SelectionSet,
            possible_type: &'a MetaType,
        ) {
            let current_ty = possible_type.name.as_str();

            for selection in &selection_set.items {
                match &selection.node {
                    Selection::Field(field) => {
                        resolver.build_field(
                            path,
                            selection_ref_set_group.entry(current_ty).or_default(),
                            fetch_entity_group,
                            current_service,
                            possible_type,
                            &field.node,
                        );
                    },
                    Selection::FragmentSpread(fragment_spread) => {
                        if let Some(fragment) = resolver.context.fragments.get(&fragment_spread.node.fragment_name.node)
                        {
                            if fragment.node.type_condition.node.on.node == current_ty {
                                build_fields(
                                    resolver,
                                    path,
                                    selection_ref_set_group,
                                    fetch_entity_group,
                                    current_service,
                                    &fragment.node.selection_set.node,
                                    possible_type,
                                );
                            } else {
                                let field_type = match resolver
                                    .context
                                    .schema
                                    .types
                                    .get(&fragment.node.type_condition.node.on.node)
                                {
                                    Some(field_type) => field_type,
                                    None => return,
                                };

                                if matches!(
                                    field_type.kind,
                                    graphgate_schema::TypeKind::Interface | graphgate_schema::TypeKind::Union
                                ) {
                                    build_fields(
                                        resolver,
                                        path,
                                        selection_ref_set_group,
                                        fetch_entity_group,
                                        current_service,
                                        &fragment.node.selection_set.node,
                                        possible_type,
                                    );
                                }
                            }
                        }
                    },
                    Selection::InlineFragment(inline_fragment) => {
                        match inline_fragment.node.type_condition.as_ref().map(|node| &node.node) {
                            Some(type_condition) if type_condition.on.node == current_ty => {
                                build_fields(
                                    resolver,
                                    path,
                                    selection_ref_set_group,
                                    fetch_entity_group,
                                    current_service,
                                    &inline_fragment.node.selection_set.node,
                                    possible_type,
                                );
                            },
                            Some(_type_condition) => {
                                // Other type condition
                            },
                            None => {
                                build_fields(
                                    resolver,
                                    path,
                                    selection_ref_set_group,
                                    fetch_entity_group,
                                    current_service,
                                    &inline_fragment.node.selection_set.node,
                                    possible_type,
                                );
                            },
                        }
                    },
                }
            }
        }

        let mut selection_ref_set_group = IndexMap::new();
        for possible_type in &parent_type.possible_types {
            if let Some(ty) = self.context.schema.types.get(possible_type) {
                path.last_mut().unwrap().possible_type = Some(ty.name.as_str());
                build_fields(
                    self,
                    path,
                    &mut selection_ref_set_group,
                    fetch_entity_group,
                    current_service,
                    selection_set,
                    ty,
                );
                path.last_mut().unwrap().possible_type = None;
            }
        }

        for (ty, sub_selection_ref_set) in selection_ref_set_group
            .into_iter()
            .filter(|(_, selection_ref_set)| !selection_ref_set.0.is_empty())
        {
            selection_ref_set.0.push(SelectionRef::InlineFragment {
                type_condition: Some(ty),
                selection_set: sub_selection_ref_set,
            });
        }
    }
}
