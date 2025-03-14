use graphgate_schema::{KeyFields, MetaField, MetaType};
use parser::types::Field;
use std::collections::HashMap;

use super::context::Context;
use crate::{
    plan::ResponsePath,
    types::{FetchEntityGroup, SelectionRefSet},
};

/// Trait for directive handlers
pub trait DirectiveHandlerTrait<'a> {
    /// Get the name of the directive
    fn name(&self) -> &'static str;

    /// Handle the directive
    fn handle(
        &mut self,
        context: &mut Context<'a>,
        field: &'a Field,
        field_definition: &'a MetaField,
        parent_type: &'a MetaType,
        current_service: &'a str,
        directive_args: &'a KeyFields,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        path: &mut ResponsePath<'a>,
    );
}

/// Registry for directive handlers
pub struct DirectiveRegistry<'a> {
    handlers: HashMap<&'static str, Box<dyn DirectiveHandlerTrait<'a> + 'a>>,
}

impl<'a> DirectiveRegistry<'a> {
    /// Create a new directive registry
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a directive handler
    pub fn register(&mut self, handler: Box<dyn DirectiveHandlerTrait<'a> + 'a>) {
        let name = handler.name();
        self.handlers.insert(name, handler);
    }

    /// Get a mutable directive handler by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Box<dyn DirectiveHandlerTrait<'a> + 'a>> {
        self.handlers.get_mut(name)
    }
}
