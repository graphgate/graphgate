use graphgate_schema::{ComposedSchema, KeyFields};
use parser::{
    types::{BaseType, FragmentDefinition, Type},
    Positioned,
};
use std::collections::HashMap;
use value::{Name, Variables};

/// Context for building a query plan
#[derive(Debug)]
pub struct Context<'a> {
    /// The composed schema
    pub schema: &'a ComposedSchema,
    /// Fragment definitions from the query
    pub fragments: &'a HashMap<Name, Positioned<FragmentDefinition>>,
    /// Variables from the query
    pub variables: &'a Variables,
    /// Counter for generating unique key prefixes
    pub key_id: usize,
}

impl<'a> Context<'a> {
    /// Create a new context
    pub fn new(
        schema: &'a ComposedSchema,
        fragments: &'a HashMap<Name, Positioned<FragmentDefinition>>,
        variables: &'a Variables,
    ) -> Self {
        Self {
            schema,
            fragments,
            variables,
            key_id: 1,
        }
    }

    /// Generate a unique key prefix
    pub fn take_key_prefix(&mut self) -> usize {
        let id = self.key_id;
        self.key_id += 1;
        id
    }

    /// Get the named type from a Type (unwrapping List and NonNull)
    pub fn get_named_type<'b>(&self, ty: &'b Type) -> Option<&'b str> {
        match &ty.base {
            BaseType::Named(name) => Some(name.as_str()),
            BaseType::List(inner) => Self::get_named_type_static(inner),
        }
    }

    /// Static version of get_named_type that doesn't require self
    pub fn get_named_type_static(ty: &Type) -> Option<&str> {
        match &ty.base {
            BaseType::Named(name) => Some(name.as_str()),
            BaseType::List(inner) => Self::get_named_type_static(inner),
        }
    }

    /// Check if a field can be provided by a service
    pub fn can_field_be_provided(
        &self,
        parent_type: &'a graphgate_schema::MetaType,
        field: &'a parser::types::Field,
        current_service: &'a str,
        selection_set: &'a parser::types::SelectionSet,
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

    /// Check if a selection set is satisfied by a @provides directive
    pub fn selection_set_satisfied_by_provides(
        &self,
        field_name: &str,
        selection_set: &'a parser::types::SelectionSet,
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

    /// Check if all selections in a selection set are satisfied by provided fields
    pub fn all_selections_satisfied(
        &self,
        selection_set: &'a parser::types::SelectionSet,
        provided_fields: &KeyFields,
    ) -> bool {
        use parser::types::Selection;

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

    /// Check if a field is in the keys
    pub fn field_in_keys(&self, field: &parser::types::Field, keys: &KeyFields) -> bool {
        use parser::types::Selection;

        fn selection_set_in_keys(
            ctx: &Context<'_>,
            selection_set: &parser::types::SelectionSet,
            keys: &KeyFields,
        ) -> bool {
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
