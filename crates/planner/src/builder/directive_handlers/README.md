# Directive Registry Pattern

This directory contains implementations of the Directive Registry Pattern for handling GraphQL directives in the GraphGate planner.

## Overview

The Directive Registry Pattern is a design pattern that enhances the modularity and maintainability of directive handling in the GraphGate planner. It replaces the monolithic approach with individual handlers for each directive type.

## Benefits

- **Modularity**: Each directive handler is in its own file, making the code more modular and easier to maintain.
- **Reusability**: The `DirectiveHandlerTrait` provides a common interface for all directive handlers.
- **Extensibility**: New directive handlers can be easily added by implementing the trait.
- **Testability**: Individual handlers can be tested in isolation.

## Implemented Directive Handlers

- `RequiresDirectiveHandler`: Handles the `@requires` directive, which specifies fields from an entity that must be fetched from another service before resolving a field.
- `ProvidesDirectiveHandler`: Handles the `@provides` directive, which indicates that a field can fetch specific subfields of an entity defined in another service.
- `TagDirectiveHandler`: Handles the `@tag` directive, which adds metadata to a field or type.

## Directive Types

Directives in GraphQL Federation can be categorized into two types:

1. **Runtime Directives**: Directives like `@requires` and `@provides` that affect query planning and execution.
2. **Schema Composition Directives**: Directives like `@shareable` and `@tag` that primarily affect schema composition.

The implementation approach for each directive handler should reflect this distinction.

## Adding a New Directive Handler

To add a new directive handler:

1. Create a new file in this directory (e.g., `my_directive_handler.rs`).
2. Implement the `DirectiveHandlerTrait` for your handler.
3. Update `mod.rs` to include your new module and export your handler.
4. Register your handler in the `FieldResolver::new` method.

## Example Implementation

```rust
use graphgate_schema::{KeyFields, MetaField, MetaType};
use parser::types::Field;

use crate::{
    builder::{
        context::Context,
        directive_registry::DirectiveHandlerTrait,
        utils::is_list,
    },
    plan::{PathSegment, ResponsePath},
    types::{FetchEntityGroup, FieldRef, SelectionRef, SelectionRefSet},
};

/// Handler for the @my_directive directive
pub struct MyDirectiveHandler;

impl MyDirectiveHandler {
    /// Create a new my_directive handler
    pub fn new() -> Self {
        Self
    }

    /// Process a field with the @my_directive directive
    fn process_my_directive_field<'a>(
        &mut self,
        context: &mut Context<'a>,
        field: &'a Field,
        field_definition: &'a MetaField,
        parent_type: &'a MetaType,
        current_service: &'a str,
        my_directive_fields: &'a KeyFields,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        path: &mut ResponsePath<'a>,
    ) {
        // Implementation goes here
    }
}

impl<'a> DirectiveHandlerTrait<'a> for MyDirectiveHandler {
    fn name(&self) -> &'static str {
        "my_directive"
    }

    fn handle(
        &mut self,
        context: &mut Context<'a>,
        field: &'a Field,
        field_definition: &'a MetaField,
        parent_type: &'a MetaType,
        current_service: &'a str,
        my_directive_fields: &'a KeyFields,
        selection_ref_set: &mut SelectionRefSet<'a>,
        fetch_entity_group: &mut FetchEntityGroup<'a>,
        path: &mut ResponsePath<'a>,
    ) {
        self.process_my_directive_field(
            context,
            field,
            field_definition,
            parent_type,
            current_service,
            my_directive_fields,
            selection_ref_set,
            fetch_entity_group,
            path,
        );
    }
}
```
