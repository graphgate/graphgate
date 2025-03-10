#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use graphgate_schema::{ComposedSchema, KeyFields, MetaField, MetaType, TypeKind, ValueExt};
use indexmap::IndexMap;
use parser::{
    types::{
        BaseType,
        DocumentOperations,
        ExecutableDocument,
        Field,
        FragmentDefinition,
        OperationDefinition,
        OperationType,
        Selection,
        SelectionSet,
        Type,
        VariableDefinition,
    },
    Positioned,
};
use tracing::instrument;
use value::{ConstValue, Name, Value, Variables};

use crate::{
    plan::{
        FetchNode,
        FlattenNode,
        IntrospectionDirective,
        IntrospectionField,
        IntrospectionNode,
        IntrospectionSelectionSet,
        ParallelNode,
        PathSegment,
        PlanNode,
        ResponsePath,
        SequenceNode,
    },
    types::{
        FetchEntity,
        FetchEntityGroup,
        FetchEntityKey,
        FetchQuery,
        FieldRef,
        MutationRootGroup,
        QueryRootGroup,
        RequiredRef,
        RootGroup,
        SelectionRef,
        SelectionRefSet,
        VariableDefinitionsRef,
        VariablesRef,
    },
    Response,
    RootNode,
    ServerError,
    SubscribeNode,
};

#[derive(Debug)]
struct Context<'a> {
    schema: &'a ComposedSchema,
    fragments: &'a HashMap<Name, Positioned<FragmentDefinition>>,
    variables: &'a Variables,
    key_id: usize,
}

/// Query plan generator
pub struct PlanBuilder<'a> {
    schema: &'a ComposedSchema,
    document: ExecutableDocument,
    operation_name: Option<String>,
    variables: Variables,
}

impl<'a> PlanBuilder<'a> {
    pub fn new(schema: &'a ComposedSchema, document: ExecutableDocument) -> Self {
        Self {
            schema,
            document,
            operation_name: None,
            variables: Default::default(),
        }
    }

    pub fn operation_name(mut self, operation: impl Into<String>) -> Self {
        self.operation_name = Some(operation.into());
        self
    }

    pub fn variables(self, variables: Variables) -> Self {
        Self { variables, ..self }
    }

    #[instrument(err(Debug), skip(self), ret, level = "trace")]
    fn check_rules(&self) -> Result<(), Response> {
        let rule_errors = graphgate_validation::check_rules(self.schema, &self.document, &self.variables);
        if !rule_errors.is_empty() {
            return Err(Response {
                data: ConstValue::Null,
                errors: rule_errors
                    .into_iter()
                    .map(|err| ServerError {
                        message: err.message,
                        path: Default::default(),
                        locations: err.locations,
                        extensions: Default::default(),
                    })
                    .collect(),
                extensions: Default::default(),
                headers: Default::default(),
            });
        }
        Ok(())
    }

    fn create_context(&self) -> Context<'_> {
        let fragments = &self.document.fragments;
        Context {
            schema: self.schema,
            fragments,
            variables: &self.variables,
            key_id: 1,
        }
    }

    #[instrument(err(Debug), skip(self), ret, level = "trace")]
    pub fn plan(&self) -> Result<RootNode, Response> {
        self.check_rules()?;

        let mut ctx = self.create_context();
        let operation_definition = get_operation(&self.document, self.operation_name.as_deref());

        let root_type = match operation_definition.node.ty {
            OperationType::Query => ctx.schema.query_type(),
            OperationType::Mutation => ctx
                .schema
                .mutation_type()
                .expect("The query validator should find this error."),
            OperationType::Subscription => ctx
                .schema
                .subscription_type()
                .expect("The query validator should find this error."),
        };

        if let Some(root_type) = ctx.schema.types.get(root_type) {
            match operation_definition.node.ty {
                OperationType::Query => Ok(RootNode::Query(ctx.build_root_selection_set(
                    QueryRootGroup::default(),
                    operation_definition.node.ty,
                    &operation_definition.node.variable_definitions,
                    root_type,
                    &operation_definition.node.selection_set.node,
                ))),
                OperationType::Mutation => Ok(RootNode::Query(ctx.build_root_selection_set(
                    MutationRootGroup::default(),
                    operation_definition.node.ty,
                    &operation_definition.node.variable_definitions,
                    root_type,
                    &operation_definition.node.selection_set.node,
                ))),
                OperationType::Subscription => Ok(RootNode::Subscribe(ctx.build_subscribe(
                    &operation_definition.node.variable_definitions,
                    root_type,
                    &operation_definition.node.selection_set.node,
                ))),
            }
        } else {
            unreachable!("The query validator should find this error.")
        }
    }
}

impl<'a> Context<'a> {
    fn build_root_selection_set(
        &mut self,
        mut root_group: impl RootGroup<'a>,
        operation_type: OperationType,
        variable_definitions: &'a [Positioned<VariableDefinition>],
        parent_type: &'a MetaType,
        selection_set: &'a SelectionSet,
    ) -> PlanNode<'a> {
        fn build_root_selection_set_rec<'a>(
            ctx: &mut Context<'a>,
            root_group: &mut impl RootGroup<'a>,
            fetch_entity_group: &mut FetchEntityGroup<'a>,
            inspection_selection_set: &mut IntrospectionSelectionSet,
            parent_type: &'a MetaType,
            selection_set: &'a SelectionSet,
        ) {
            for selection in &selection_set.items {
                match &selection.node {
                    Selection::Field(field) => {
                        let field_name = field.node.name.node.as_str();
                        let field_definition = match parent_type.fields.get(field_name) {
                            Some(field_definition) => field_definition,
                            None => continue,
                        };
                        if is_introspection_field(field_name) {
                            ctx.build_introspection_field(inspection_selection_set, &field.node);
                            continue;
                        }

                        if let Some(service) = &field_definition.service {
                            let selection_ref_set = root_group.selection_set_mut(service);
                            let mut path = ResponsePath::default();
                            ctx.build_field(
                                &mut path,
                                selection_ref_set,
                                fetch_entity_group,
                                service,
                                parent_type,
                                &field.node,
                            );
                        }
                    },
                    Selection::FragmentSpread(fragment_spread) => {
                        if let Some(fragment) = ctx.fragments.get(fragment_spread.node.fragment_name.node.as_str()) {
                            build_root_selection_set_rec(
                                ctx,
                                root_group,
                                fetch_entity_group,
                                inspection_selection_set,
                                parent_type,
                                &fragment.node.selection_set.node,
                            );
                        }
                    },
                    Selection::InlineFragment(inline_fragment) => {
                        build_root_selection_set_rec(
                            ctx,
                            root_group,
                            fetch_entity_group,
                            inspection_selection_set,
                            parent_type,
                            &inline_fragment.node.selection_set.node,
                        );
                    },
                }
            }
        }

        let mut fetch_entity_group = FetchEntityGroup::default();
        let mut inspection_selection_set = IntrospectionSelectionSet::default();
        build_root_selection_set_rec(
            self,
            &mut root_group,
            &mut fetch_entity_group,
            &mut inspection_selection_set,
            parent_type,
            selection_set,
        );

        let mut nodes = Vec::new();
        if !inspection_selection_set.0.is_empty() {
            nodes.push(PlanNode::Introspection(IntrospectionNode {
                selection_set: inspection_selection_set,
            }));
        }

        let fetch_node = {
            let mut nodes = Vec::new();
            for (service, selection_set) in root_group.into_selection_set() {
                let (variables, variable_definitions) =
                    referenced_variables(&selection_set, self.variables, variable_definitions);
                nodes.push(PlanNode::Fetch(FetchNode {
                    service,
                    variables,
                    query: FetchQuery {
                        entity_type: None,
                        operation_type,
                        variable_definitions,
                        selection_set,
                    },
                }));
            }
            if operation_type == OperationType::Query {
                PlanNode::Parallel(ParallelNode { nodes }).flatten()
            } else {
                PlanNode::Sequence(SequenceNode { nodes }).flatten()
            }
        };
        nodes.push(fetch_node);

        while !fetch_entity_group.is_empty() {
            let mut flatten_nodes = Vec::new();
            let mut next_group = FetchEntityGroup::new();

            for (
                FetchEntityKey { service, mut path, .. },
                FetchEntity {
                    parent_type,
                    prefix,
                    fields,
                },
            ) in fetch_entity_group
            {
                let mut selection_ref_set = SelectionRefSet::default();

                for field in fields {
                    self.build_field(
                        &mut path,
                        &mut selection_ref_set,
                        &mut next_group,
                        service,
                        parent_type,
                        field,
                    );
                }

                let (variables, variable_definitions) =
                    referenced_variables(&selection_ref_set, self.variables, variable_definitions);
                flatten_nodes.push(PlanNode::Flatten(FlattenNode {
                    path,
                    prefix,
                    service,
                    variables,
                    query: FetchQuery {
                        entity_type: Some(parent_type.name.as_str()),
                        operation_type: OperationType::Subscription,
                        variable_definitions,
                        selection_set: selection_ref_set,
                    },
                }));
            }

            nodes.push(PlanNode::Parallel(ParallelNode { nodes: flatten_nodes }).flatten());
            fetch_entity_group = next_group;
        }

        PlanNode::Sequence(SequenceNode { nodes }).flatten()
    }

    fn build_subscribe(
        &mut self,
        variable_definitions: &'a [Positioned<VariableDefinition>],
        parent_type: &'a MetaType,
        selection_set: &'a SelectionSet,
    ) -> SubscribeNode<'a> {
        let mut root_group = QueryRootGroup::default();
        let mut fetch_entity_group = FetchEntityGroup::default();

        for selection in &selection_set.items {
            if let Selection::Field(field) = &selection.node {
                let field_name = field.node.name.node.as_str();
                let field_definition = match parent_type.fields.get(field_name) {
                    Some(field_definition) => field_definition,
                    None => continue,
                };

                if let Some(service) = &field_definition.service {
                    let selection_ref_set = root_group.selection_set_mut(service);
                    let mut path = ResponsePath::default();
                    self.build_field(
                        &mut path,
                        selection_ref_set,
                        &mut fetch_entity_group,
                        service,
                        parent_type,
                        &field.node,
                    );
                }
            }
        }

        let fetch_nodes = {
            let mut nodes = Vec::new();
            for (service, selection_ref_set) in root_group.into_selection_set() {
                let (variables, variable_definitions) =
                    referenced_variables(&selection_ref_set, self.variables, variable_definitions);
                nodes.push(FetchNode {
                    service,
                    variables,
                    query: FetchQuery {
                        entity_type: None,
                        operation_type: OperationType::Subscription,
                        variable_definitions,
                        selection_set: selection_ref_set,
                    },
                });
            }
            nodes
        };

        let mut query_nodes = Vec::new();
        while !fetch_entity_group.is_empty() {
            let mut flatten_nodes = Vec::new();
            let mut next_group = FetchEntityGroup::new();

            for (
                FetchEntityKey { service, mut path, .. },
                FetchEntity {
                    parent_type,
                    prefix,
                    fields,
                },
            ) in fetch_entity_group
            {
                let mut selection_ref_set = SelectionRefSet::default();

                for field in fields {
                    self.build_field(
                        &mut path,
                        &mut selection_ref_set,
                        &mut next_group,
                        service,
                        parent_type,
                        field,
                    );
                }

                let (variables, variable_definitions) =
                    referenced_variables(&selection_ref_set, self.variables, variable_definitions);
                flatten_nodes.push(PlanNode::Flatten(FlattenNode {
                    path,
                    prefix,
                    service,
                    variables,
                    query: FetchQuery {
                        entity_type: Some(parent_type.name.as_str()),
                        operation_type: OperationType::Query,
                        variable_definitions,
                        selection_set: selection_ref_set,
                    },
                }));
            }

            query_nodes.push(PlanNode::Parallel(ParallelNode { nodes: flatten_nodes }).flatten());
            fetch_entity_group = next_group;
        }

        SubscribeNode {
            subscribe_nodes: fetch_nodes,
            flatten_node: if query_nodes.is_empty() {
                None
            } else {
                Some(PlanNode::Sequence(SequenceNode { nodes: query_nodes }).flatten())
            },
        }
    }

    fn build_introspection_field(
        &mut self,
        introspection_selection_set: &mut IntrospectionSelectionSet,
        field: &'a Field,
    ) {
        fn build_selection_set<'a>(
            ctx: &mut Context<'a>,
            introspection_selection_set: &mut IntrospectionSelectionSet,
            selection_set: &'a SelectionSet,
        ) {
            for selection in &selection_set.items {
                match &selection.node {
                    Selection::Field(field) => {
                        ctx.build_introspection_field(introspection_selection_set, &field.node);
                    },
                    Selection::FragmentSpread(fragment_spread) => {
                        if let Some(fragment) = ctx.fragments.get(fragment_spread.node.fragment_name.node.as_str()) {
                            build_selection_set(ctx, introspection_selection_set, &fragment.node.selection_set.node);
                        }
                    },
                    Selection::InlineFragment(inline_fragment) => {
                        build_selection_set(
                            ctx,
                            introspection_selection_set,
                            &inline_fragment.node.selection_set.node,
                        );
                    },
                }
            }
        }

        fn convert_arguments(
            ctx: &mut Context,
            arguments: &[(Positioned<Name>, Positioned<Value>)],
        ) -> IndexMap<Name, ConstValue> {
            arguments
                .iter()
                .map(|(name, value)| {
                    (
                        name.node.clone(),
                        value
                            .node
                            .clone()
                            .into_const_with(|name| {
                                Ok::<_, std::convert::Infallible>(ctx.variables.get(&name).unwrap().clone())
                            })
                            .unwrap(),
                    )
                })
                .collect()
        }

        let mut sub_selection_set = IntrospectionSelectionSet::default();
        build_selection_set(self, &mut sub_selection_set, &field.selection_set.node);
        introspection_selection_set.0.push(IntrospectionField {
            name: field.name.node.clone(),
            alias: field.alias.clone().map(|alias| alias.node),
            arguments: convert_arguments(self, &field.arguments),
            directives: field
                .directives
                .iter()
                .map(|directive| IntrospectionDirective {
                    name: directive.node.name.node.clone(),
                    arguments: convert_arguments(self, &directive.node.arguments),
                })
                .collect(),
            selection_set: sub_selection_set,
        });
    }

    fn build_field(
        &mut self,
        path: &mut ResponsePath<'a>,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        current_service: &'a str,
        parent_type: &'a MetaType,
        field: &'a Field,
    ) {
        // Get the field definition from the parent type
        let field_definition = match parent_type.field_by_name(field.name.node.as_str()) {
            Some(field_definition) => field_definition,
            None => return,
        };

        // Check if the field has a specific service defined
        let field_service = field_definition.service.as_deref();

        // First check if the field can be provided by the current service via @provides
        let can_be_provided =
            self.can_field_be_provided(parent_type, field, current_service, &field.selection_set.node);

        // Determine the service to use for this field
        // Prefer the current service if it can provide the field to minimize service hops
        // If the field has a specific service defined, use that service
        let service = if let Some(service) = field_service {
            // If the field has a specific service defined, use that service
            service
        } else if can_be_provided {
            current_service
        } else {
            match parent_type.owner.as_deref() {
                Some(service) => service,
                None => current_service,
            }
        };

        // Check if this field has a @requires directive
        if let Some(requires) = &field_definition.requires {
            // Only process @requires if we're in the service that owns the field
            if service == current_service {
                // Handle the @requires directive
                self.handle_requires_directive(
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
            }
        }

        if service != current_service {
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
                None => return,
            };

            // Check if the field is defined in the service
            // If the field is defined in the service but not in the keys, we need to fetch it
            let field_defined_in_service = field_definition.service.as_deref() == Some(service);

            // Check if the field has a @provides directive
            // If the field has a @provides directive, we need to fetch it from the service that defines it
            let field_has_provides = field_definition.provides.is_some();
            
            // Check if the @provides directive can actually satisfy the requested fields
            let provides_can_satisfy = if let Some(provides) = &field_definition.provides {
                self.selection_set_satisfied_by_provides(field.name.node.as_str(), &field.selection_set.node, provides)
            } else {
                false
            };

            // If the field is defined in the service, has a valid @provides directive, or not in the keys, we need to fetch
            // it from the current service
            if field_defined_in_service || (field_has_provides && provides_can_satisfy) || !self.field_in_keys(field, keys) {
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
                    }
                };
                
                // Find the service that owns the field's type
                if let Some(field_type) = self.schema.types.get(field_type_name) {
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
            return;
        }

        path.push(PathSegment {
            name: field.response_key().node.as_str(),
            is_list: is_list(&field_definition.ty),
            possible_type: None,
        });

        // Create a selection set for this field
        let mut sub_selection_set = SelectionRefSet::default();

        // If the field has a selection set, build it
        if !field.selection_set.node.items.is_empty() {
            if let Some(field_type) = self.schema.get_type(&field_definition.ty) {
                if matches!(field_type.kind, TypeKind::Interface | TypeKind::Union) {
                    self.build_abstract_selection_set(
                        path,
                        &mut sub_selection_set,
                        fetch_entity_group,
                        service,
                        field_type,
                        &field.selection_set.node,
                    );
                } else {
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

    // Method to handle fields with the @requires directive
    fn handle_requires_directive(
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
        // Check if the path length exceeds a reasonable limit to prevent infinite loops
        if path.len() > 10 {
            // Count occurrences of each segment to detect potential loops
            let mut segment_counts = std::collections::HashMap::new();
            for segment in path.iter() {
                *segment_counts.entry(segment.name).or_insert(0) += 1;
                // If any segment appears more than 3 times, we likely have a loop
                if *segment_counts.get(segment.name).unwrap() > 3 {
                    return;
                }
            }
        }

        // Get the entity field name from the requires directive
        if let Some(entity_field_name) = requires.keys().next() {
            // Find the entity field definition in the parent type
            if let Some(entity_field_definition) = parent_type.fields.get(entity_field_name) {
                // Get the entity type from the field definition
                if let Some(entity_type) = self.schema.get_type(&entity_field_definition.ty) {
                    // Find the service that owns the entity
                    let entity_service = if let Some(owner) = entity_type.owner.as_deref() {
                        owner
                    } else {
                        // If owner is None, try to determine the service from the keys and required fields
                        // For the @requires directive, we need to find the service that can provide the required fields
                        
                        // Check which services have keys for this entity
                        if entity_type.keys.is_empty() {
                            // If no services have keys for this entity, we can't handle the @requires directive
                            return;
                        }
                        
                        // For the @requires directive, we need to find the service that can provide the required fields
                        
                        // Extract the required field names from the requires directive
                        let _required_field_names: Vec<&str> = requires
                            .values()
                            .flat_map(|fields| fields.keys())
                            .map(|name| name.as_str())
                            .collect();
                        
                        // Try to find the best service to provide the required fields
                        // Strategy:
                        // 1. Look for a service that has all the required fields
                        // 2. If no service has all the required fields, use the first service with keys
                        
                        // First, try to find a service that has all the required fields
                        // This is a heuristic based on the assumption that a service that has keys for an entity
                        // and is not the current service is likely to be able to provide the required fields
                        let mut found_service = None;
                        for (service_name, _) in &entity_type.keys {
                            let service_name_str = service_name.as_str();
                            // Skip the current service, as we're looking for a different service
                            // that can provide the required fields
                            if service_name_str != current_service {
                                found_service = Some(service_name_str);
                                break;
                            }
                        }
                        
                        // If we found a suitable service, use it
                        if let Some(service) = found_service {
                            service
                        } else if let Some(first_service) = entity_type.keys.keys().next() {
                            // Otherwise, use the first service with keys
                            first_service.as_str()
                        } else {
                            // This should never happen because we checked if keys is empty above
                            // But just in case, return the current service
                            current_service
                        }
                    };
                    
                    // Now we have determined the entity service, continue with the rest of the function
                    
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

                    // Get the current key ID for this entity
                    let prefix = self.take_key_prefix();

                    // Add a RequiredRef to indicate that this field requires additional fields
                    // This is crucial for the federation gateway to understand the dependencies
                    selection_ref_set.0.push(SelectionRef::RequiredRef(RequiredRef {
                        prefix,
                        fields: requires,
                        requires: Some(requires),
                    }));

                    // Create a fetch entity key for the current service and field
                    let fetch_entity_key = FetchEntityKey {
                        service: current_service,
                        path: path.clone(),
                        ty: parent_type.name.as_str(),
                    };

                    // Check if this entity already exists in the fetch group
                    if let Some(fetch_entity) = fetch_entity_group.get_mut(&fetch_entity_key) {
                        // Remove the field with @requires from the entity's fields to prevent
                        // it from being fetched before the required fields are available
                        fetch_entity.fields.retain(|f| f.name.node != field.name.node);
                    }

                    // Create a separate fetch entity for the field with @requires
                    // This will be executed after the required fields are fetched
                    fetch_entity_group.insert(fetch_entity_key, FetchEntity {
                        parent_type,
                        prefix,
                        fields: vec![field],
                    });

                    // Now, we need to ensure the required fields are fetched from the entity's service
                    // Create a path for the entity field
                    let mut entity_path = path.clone();
                    entity_path.pop(); // Remove the current field
                    
                    // Create a fetch entity key for the entity
                    let entity_fetch_key = FetchEntityKey {
                        service: entity_service,
                        path: entity_path,
                        ty: entity_type.name.as_str(),
                    };

                    // Check if the entity already exists in the fetch group
                    let entity_exists = fetch_entity_group.contains_key(&entity_fetch_key);
                    
                    if !entity_exists {
                        // Find the key fields for the entity
                        if let Some(_keys) = entity_type.keys.get(entity_service).and_then(|x| x.first()) {
                            // Add the entity to the fetch group with an empty fields vector
                            // This ensures the entity is fetched from its service
                            fetch_entity_group.insert(entity_fetch_key, FetchEntity {
                                parent_type: entity_type,
                                prefix,
                                fields: vec![],
                            });
                        }
                    }

                    // Pop the path segment we pushed earlier
                    path.pop();
                    return;
                }
            }
        }
    }

    // New helper function to check if a field can be provided by the current service
    fn can_field_be_provided(
        &self,
        parent_type: &'a MetaType,
        field: &'a Field,
        current_service: &'a str,
        selection_set: &'a SelectionSet,
    ) -> bool {
        // Check all fields in the parent type to see if any of them have a @provides directive
        // that can satisfy the requested field
        for (_, meta_field) in &parent_type.fields {
            // Skip fields that don't belong to the current service
            if meta_field.service.as_deref() != Some(current_service) &&
                parent_type.owner.as_deref() != Some(current_service)
            {
                continue;
            }

            // Check if this field has a @provides directive
            if let Some(provides) = &meta_field.provides {
                // Check if the provided fields can satisfy the requested field's selection set
                if self.selection_set_satisfied_by_provides(field.name.node.as_str(), selection_set, provides) {
                    return true;
                }
            }
        }
        false
    }

    // Helper function to check if a selection set is satisfied by a @provides directive
    fn selection_set_satisfied_by_provides(
        &self,
        field_name: &str,
        selection_set: &'a SelectionSet,
        provides: &KeyFields,
    ) -> bool {
        // Check if the field is directly provided
        if provides.contains_key(field_name) {
            // If the field has a selection set, we need to check if all requested fields are provided
            if !selection_set.items.is_empty() {
                if let Some(provided_fields) = provides.get(field_name) {
                    return self.all_selections_satisfied(selection_set, provided_fields);
                }
                return false;
            }
            return true;
        }
        false
    }

    // Helper function to check if all selections in a selection set are satisfied by provided fields
    fn all_selections_satisfied(&self, selection_set: &'a SelectionSet, provided_fields: &KeyFields) -> bool {
        for selection in &selection_set.items {
            match &selection.node {
                Selection::Field(field) => {
                    let field_name = field.node.name.node.as_str();

                    // Skip __typename as it's always available
                    if field_name == "__typename" {
                        continue;
                    }

                    // Check if the field is provided
                    if !provided_fields.contains_key(field_name) {
                        return false;
                    }

                    // If this field has a selection set, recursively check it
                    if !field.node.selection_set.node.items.is_empty() {
                        if let Some(sub_provided_fields) = provided_fields.get(field_name) {
                            if !self.all_selections_satisfied(&field.node.selection_set.node, sub_provided_fields) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                },
                // Handle fragment spreads by checking the fragment's selection set
                Selection::FragmentSpread(fragment_spread) => {
                    if let Some(fragment) = self.fragments.get(&fragment_spread.node.fragment_name.node) {
                        // Check if all selections in the fragment are satisfied
                        if !self.all_selections_satisfied(&fragment.node.selection_set.node, provided_fields) {
                            return false;
                        }
                    } else {
                        // If we can't find the fragment, we can't guarantee it's satisfied
                        return false;
                    }
                },
                // Handle inline fragments by checking their selection sets
                Selection::InlineFragment(inline_fragment) => {
                    // For inline fragments, we need to check if the type condition is compatible
                    // with the provided fields. For simplicity, we'll just check the selection set.
                    if !self.all_selections_satisfied(&inline_fragment.node.selection_set.node, provided_fields) {
                        return false;
                    }
                },
            }
        }
        true
    }

    fn add_fetch_entity(
        &mut self,
        field: &'a Field,
        field_definition: &'a MetaField,
        parent_type: &'a MetaType,
        service: &'a str,
        _selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        path: &mut ResponsePath<'a>,
    ) {
        // Add the field to the path for proper tracking
        path.push(PathSegment {
            name: field.response_key().node.as_str(),
            is_list: is_list(&field_definition.ty),
            possible_type: None,
        });

        // Create a fetch entity key for this field
        let fetch_entity_key = FetchEntityKey {
            service,
            path: path.clone(),
            ty: parent_type.name.as_str(),
        };

        // Check if this entity already exists in the fetch group
        let entity_exists = fetch_entity_group.contains_key(&fetch_entity_key);
        
        if !entity_exists {
            // Create a new entity with a unique prefix
            let prefix = self.take_key_prefix();
            
            // Add the entity to the fetch group
            fetch_entity_group.insert(fetch_entity_key, FetchEntity {
                parent_type,
                prefix,
                fields: vec![field],
            });
        } else {
            // Add the field to the existing entity
            if let Some(fetch_entity) = fetch_entity_group.get_mut(&fetch_entity_key) {
                fetch_entity.fields.push(field);
            }
        }

        // Pop the path segment we pushed earlier
        path.pop();
    }

    fn build_selection_set(
        &mut self,
        path: &mut ResponsePath<'a>,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        current_service: &'a str,
        parent_type: &'a MetaType,
        selection_set: &'a SelectionSet,
    ) {
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
                    if let Some(fragment) = self.fragments.get(fragment_spread.node.fragment_name.node.as_str()) {
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

    fn build_abstract_selection_set(
        &mut self,
        path: &mut ResponsePath<'a>,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        current_service: &'a str,
        parent_type: &'a MetaType,
        selection_set: &'a SelectionSet,
    ) {
        fn build_fields<'a>(
            ctx: &mut Context<'a>,
            path: &mut ResponsePath<'a>,
            selection_ref_set_group: &mut IndexMap<&'a str, SelectionRefSet<'a>>,
            fetch_entity_group: &mut FetchEntityGroup<'a>,
            current_service: &'a str,
            selection_set: &'a SelectionSet,
            possible_type: &'a MetaType,
        ) {
            let current_ty = possible_type.name.as_str();

            for selection in &selection_set.items {
                match &selection.node {
                    Selection::Field(field) => {
                        ctx.build_field(
                            path,
                            selection_ref_set_group.entry(current_ty).or_default(),
                            fetch_entity_group,
                            current_service,
                            possible_type,
                            &field.node,
                        );
                    },
                    Selection::FragmentSpread(fragment_spread) => {
                        if let Some(fragment) = ctx.fragments.get(&fragment_spread.node.fragment_name.node) {
                            if fragment.node.type_condition.node.on.node == current_ty {
                                build_fields(
                                    ctx,
                                    path,
                                    selection_ref_set_group,
                                    fetch_entity_group,
                                    current_service,
                                    &fragment.node.selection_set.node,
                                    possible_type,
                                );
                            } else {
                                let field_type = match ctx.schema.types.get(&fragment.node.type_condition.node.on.node)
                                {
                                    Some(field_type) => field_type,
                                    None => return,
                                };

                                if matches!(field_type.kind, TypeKind::Interface | TypeKind::Union) {
                                    build_fields(
                                        ctx,
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
                                    ctx,
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
                                    ctx,
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
            if let Some(ty) = self.schema.types.get(possible_type) {
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

    fn take_key_prefix(&mut self) -> usize {
        let id = self.key_id;
        self.key_id += 1;
        id
    }

    fn field_in_keys(&self, field: &Field, keys: &KeyFields) -> bool {
        fn selection_set_in_keys(ctx: &Context<'_>, selection_set: &SelectionSet, keys: &KeyFields) -> bool {
            for selection in &selection_set.items {
                match &selection.node {
                    Selection::Field(field) => {
                        if !ctx.field_in_keys(&field.node, keys) {
                            return false;
                        }
                    },
                    Selection::FragmentSpread(fragment_spread) => {
                        if let Some(fragment) = ctx.fragments.get(fragment_spread.node.fragment_name.node.as_str()) {
                            if !selection_set_in_keys(ctx, &fragment.node.selection_set.node, keys) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    },
                    Selection::InlineFragment(inline_fragment) => {
                        if !selection_set_in_keys(ctx, &inline_fragment.node.selection_set.node, keys) {
                            return false;
                        }
                    },
                }
            }
            true
        }

        // Check if the field is directly in the keys
        if let Some(children) = keys.get(field.name.node.as_str()) {
            return selection_set_in_keys(self, &field.selection_set.node, children);
        }

        // For compound keys like "id username", we need to check if the field is part of the key
        // Check if the field name is one of the top-level keys in the KeyFields
        for key_name in keys.keys() {
            // For compound keys, the key name might be something like "id username"
            // Split by whitespace and check if the field name is one of the parts
            if key_name.split_whitespace().any(|part| part == field.name.node.as_str()) {
                // If the field is part of a compound key, we consider it to be in the keys
                return true;
            }
        }

        // If we get here, the field is not in the keys
        false
    }
}

#[inline]
fn is_list(ty: &Type) -> bool {
    matches!(ty.base, BaseType::List(_))
}

#[instrument(ret, level = "trace")]
fn get_operation<'a>(
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

fn referenced_variables<'a>(
    selection_set: &SelectionRefSet<'a>,
    variables: &'a Variables,
    variable_definitions: &'a [Positioned<VariableDefinition>],
) -> (VariablesRef<'a>, VariableDefinitionsRef<'a>) {
    fn referenced_variables_rec<'a>(
        selection_set: &SelectionRefSet<'a>,
        variables: &'a Variables,
        variable_definitions: &'a [Positioned<VariableDefinition>],
        variables_ref: &mut VariablesRef<'a>,
        variables_definition_ref: &mut IndexMap<&'a str, &'a VariableDefinition>,
    ) {
        for selection in &selection_set.0 {
            match selection {
                SelectionRef::FieldRef(field) => {
                    for (_, value) in &field.field.arguments {
                        for name in value.node.referenced_variables() {
                            if let Some((value, definition)) = variables
                                .get(name)
                                .zip(variable_definitions.iter().find(|d| d.node.name.node.as_str() == name))
                            {
                                variables_ref.variables.insert(name, value);
                                variables_definition_ref.insert(name, &definition.node);
                            } else {
                                let definition = variable_definitions
                                    .iter()
                                    .find(|d| d.node.name.node.as_str() == name)
                                    .unwrap();
                                variables_definition_ref.insert(name, &definition.node);
                            }
                        }
                    }

                    for dir in &field.field.directives {
                        for (_, value) in &dir.node.arguments {
                            for name in value.node.referenced_variables() {
                                if let Some((value, definition)) = variables
                                    .get(name)
                                    .zip(variable_definitions.iter().find(|d| d.node.name.node.as_str() == name))
                                {
                                    variables_ref.variables.insert(name, value);
                                    variables_definition_ref.insert(name, &definition.node);
                                } else {
                                    let definition = variable_definitions
                                        .iter()
                                        .find(|d| d.node.name.node.as_str() == name)
                                        .unwrap();
                                    variables_definition_ref.insert(name, &definition.node);
                                }
                            }
                        }
                    }
                    referenced_variables_rec(
                        &field.selection_set,
                        variables,
                        variable_definitions,
                        variables_ref,
                        variables_definition_ref,
                    )
                },

                SelectionRef::InlineFragment { selection_set, .. } => referenced_variables_rec(
                    selection_set,
                    variables,
                    variable_definitions,
                    variables_ref,
                    variables_definition_ref,
                ),
                _ => {},
            }
        }
    }

    let mut variables_ref = VariablesRef::default();
    let mut variable_definition_ref = IndexMap::new();
    referenced_variables_rec(
        selection_set,
        variables,
        variable_definitions,
        &mut variables_ref,
        &mut variable_definition_ref,
    );
    (variables_ref, VariableDefinitionsRef {
        variables: variable_definition_ref.into_iter().map(|(_, value)| value).collect(),
    })
}
#[inline]
fn is_introspection_field(name: &str) -> bool {
    name == "__type" || name == "__schema"
}

