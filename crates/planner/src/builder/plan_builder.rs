use indexmap::IndexMap;

use graphgate_schema::{ComposedSchema, MetaType, ValueExt};
use parser::{
    types::{ExecutableDocument, Field, OperationType, Selection, SelectionSet, VariableDefinition},
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
        PlanNode,
        ResponsePath,
        SequenceNode,
    },
    types::{
        FetchEntity,
        FetchEntityGroup,
        FetchEntityKey,
        FetchQuery,
        MutationRootGroup,
        QueryRootGroup,
        RootGroup,
        SelectionRefSet,
        VariableDefinitionsRef,
        VariablesRef,
    },
    Response,
    RootNode,
    ServerError,
    SubscribeNode,
};

use super::{context::Context, field_resolver::FieldResolver, utils::get_operation};

/// Query plan generator
pub struct PlanBuilder<'a> {
    schema: &'a ComposedSchema,
    document: ExecutableDocument,
    operation_name: Option<String>,
    variables: Variables,
}

impl<'a> PlanBuilder<'a> {
    /// Create a new plan builder
    pub fn new(schema: &'a ComposedSchema, document: ExecutableDocument) -> Self {
        Self {
            schema,
            document,
            operation_name: None,
            variables: Default::default(),
        }
    }

    /// Set the operation name
    pub fn operation_name(mut self, operation: impl Into<String>) -> Self {
        self.operation_name = Some(operation.into());
        self
    }

    /// Set the variables
    pub fn variables(self, variables: Variables) -> Self {
        Self { variables, ..self }
    }

    /// Check validation rules
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

    /// Create a context for building the plan
    fn create_context(&self) -> Context<'_> {
        let fragments = &self.document.fragments;
        Context::new(self.schema, fragments, &self.variables)
    }

    /// Generate a query plan
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
            // We need to clone the context to avoid lifetime issues
            let ctx_ptr = &mut ctx as *mut Context<'_>;

            match operation_definition.node.ty {
                OperationType::Query => {
                    // SAFETY: This is safe because we're just creating a new reference to the same context
                    let result = unsafe {
                        self.build_root_selection_set(
                            &mut *ctx_ptr,
                            QueryRootGroup::default(),
                            operation_definition.node.ty,
                            &operation_definition.node.variable_definitions,
                            root_type,
                            &operation_definition.node.selection_set.node,
                        )
                    };
                    Ok(RootNode::Query(result))
                },
                OperationType::Mutation => {
                    // SAFETY: This is safe because we're just creating a new reference to the same context
                    let result = unsafe {
                        self.build_root_selection_set(
                            &mut *ctx_ptr,
                            MutationRootGroup::default(),
                            operation_definition.node.ty,
                            &operation_definition.node.variable_definitions,
                            root_type,
                            &operation_definition.node.selection_set.node,
                        )
                    };
                    Ok(RootNode::Query(result))
                },
                OperationType::Subscription => {
                    // SAFETY: This is safe because we're just creating a new reference to the same context
                    let result = unsafe {
                        self.build_subscribe(
                            &mut *ctx_ptr,
                            &operation_definition.node.variable_definitions,
                            root_type,
                            &operation_definition.node.selection_set.node,
                        )
                    };
                    Ok(RootNode::Subscribe(result))
                },
            }
        } else {
            unreachable!("The query validator should find this error.")
        }
    }

    /// Build a root selection set
    fn build_root_selection_set<'b>(
        &'b self,
        ctx: &'b mut Context<'a>,
        mut root_group: impl RootGroup<'a>,
        operation_type: OperationType,
        variable_definitions: &'a [Positioned<VariableDefinition>],
        parent_type: &'a MetaType,
        selection_set: &'a SelectionSet,
    ) -> PlanNode<'a>
    where
        'b: 'a,
    {
        let mut field_resolver = FieldResolver::new(ctx);
        let mut fetch_entity_group = FetchEntityGroup::default();
        let mut inspection_selection_set = IntrospectionSelectionSet::default();

        self.build_root_selection_set_rec(
            &mut field_resolver,
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
                    self.referenced_variables(&selection_set, &self.variables, variable_definitions);
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
                    field_resolver.build_field(
                        &mut path,
                        &mut selection_ref_set,
                        &mut next_group,
                        service,
                        parent_type,
                        field,
                    );
                }

                let (variables, variable_definitions) =
                    self.referenced_variables(&selection_ref_set, &self.variables, variable_definitions);
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

    /// Build a root selection set recursively
    fn build_root_selection_set_rec<'b>(
        &'b self,
        field_resolver: &mut FieldResolver<'b>,
        root_group: &mut impl RootGroup<'b>,
        fetch_entity_group: &mut FetchEntityGroup<'b>,
        inspection_selection_set: &mut IntrospectionSelectionSet,
        parent_type: &'b MetaType,
        selection_set: &'b SelectionSet,
    ) where
        'a: 'b,
    {
        for selection in &selection_set.items {
            match &selection.node {
                Selection::Field(field) => {
                    let field_name = field.node.name.node.as_str();
                    let field_definition = match parent_type.fields.get(field_name) {
                        Some(field_definition) => field_definition,
                        None => continue,
                    };
                    if self.is_introspection_field(field_name) {
                        self.build_introspection_field(field_resolver.context, inspection_selection_set, &field.node);
                        continue;
                    }

                    if let Some(service) = &field_definition.service {
                        let selection_ref_set = root_group.selection_set_mut(service);
                        let mut path = ResponsePath::default();
                        field_resolver.build_field(
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
                    if let Some(fragment) = field_resolver
                        .context
                        .fragments
                        .get(fragment_spread.node.fragment_name.node.as_str())
                    {
                        self.build_root_selection_set_rec(
                            field_resolver,
                            root_group,
                            fetch_entity_group,
                            inspection_selection_set,
                            parent_type,
                            &fragment.node.selection_set.node,
                        );
                    }
                },
                Selection::InlineFragment(inline_fragment) => {
                    self.build_root_selection_set_rec(
                        field_resolver,
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

    /// Build a subscription plan
    fn build_subscribe<'b>(
        &'b self,
        ctx: &'b mut Context<'a>,
        variable_definitions: &'a [Positioned<VariableDefinition>],
        parent_type: &'a MetaType,
        selection_set: &'a SelectionSet,
    ) -> SubscribeNode<'a>
    where
        'b: 'a,
    {
        let mut field_resolver = FieldResolver::new(ctx);
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
                    field_resolver.build_field(
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
                    self.referenced_variables(&selection_ref_set, &self.variables, variable_definitions);
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
                    field_resolver.build_field(
                        &mut path,
                        &mut selection_ref_set,
                        &mut next_group,
                        service,
                        parent_type,
                        field,
                    );
                }

                let (variables, variable_definitions) =
                    self.referenced_variables(&selection_ref_set, &self.variables, variable_definitions);
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

    /// Build an introspection field
    fn build_introspection_field(
        &self,
        ctx: &mut Context<'a>,
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
                        build_introspection_field(ctx, introspection_selection_set, &field.node);
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

        fn build_introspection_field<'a>(
            ctx: &mut Context<'a>,
            introspection_selection_set: &mut IntrospectionSelectionSet,
            field: &'a Field,
        ) {
            let mut sub_selection_set = IntrospectionSelectionSet::default();
            build_selection_set(ctx, &mut sub_selection_set, &field.selection_set.node);
            introspection_selection_set.0.push(IntrospectionField {
                name: field.name.node.clone(),
                alias: field.alias.clone().map(|alias| alias.node),
                arguments: convert_arguments(ctx, &field.arguments),
                directives: field
                    .directives
                    .iter()
                    .map(|directive| IntrospectionDirective {
                        name: directive.node.name.node.clone(),
                        arguments: convert_arguments(ctx, &directive.node.arguments),
                    })
                    .collect(),
                selection_set: sub_selection_set,
            });
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

        build_introspection_field(ctx, introspection_selection_set, field);
    }

    /// Check if a field is an introspection field
    #[inline]
    fn is_introspection_field(&self, name: &str) -> bool {
        name == "__type" || name == "__schema"
    }

    /// Get the variables referenced in a selection set
    fn referenced_variables<'b>(
        &self,
        selection_set: &SelectionRefSet<'b>,
        variables: &'b Variables,
        variable_definitions: &'b [Positioned<VariableDefinition>],
    ) -> (VariablesRef<'b>, VariableDefinitionsRef<'b>) {
        fn referenced_variables_rec<'a>(
            selection_set: &SelectionRefSet<'a>,
            variables: &'a Variables,
            variable_definitions: &'a [Positioned<VariableDefinition>],
            variables_ref: &mut VariablesRef<'a>,
            variables_definition_ref: &mut IndexMap<&'a str, &'a VariableDefinition>,
        ) {
            for selection in &selection_set.0 {
                match selection {
                    crate::types::SelectionRef::FieldRef(field) => {
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

                    crate::types::SelectionRef::InlineFragment { selection_set, .. } => referenced_variables_rec(
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
}
