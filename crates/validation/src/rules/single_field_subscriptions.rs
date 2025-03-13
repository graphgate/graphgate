use parser::{
    types::{OperationDefinition, OperationType, Selection, SelectionSet},
    Positioned,
};
use value::Name;

use crate::{Visitor, VisitorContext};

#[derive(Default)]
pub struct SingleFieldSubscriptions;

impl<'a> Visitor<'a> for SingleFieldSubscriptions {
    fn enter_operation_definition(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        _name: Option<&'a Name>,
        operation_definition: &'a Positioned<OperationDefinition>,
    ) {
        if operation_definition.node.ty == OperationType::Subscription {
            let selection_set = &operation_definition.node.selection_set;
            let fields = count_fields(selection_set, ctx);

            if fields > 1 {
                ctx.report_error(
                    vec![selection_set.pos],
                    "Subscription operations must have exactly one root field".to_string(),
                );
            }
        }
    }
}

fn count_fields<'a>(selection_set: &'a Positioned<SelectionSet>, ctx: &'a VisitorContext<'a>) -> usize {
    let mut count = 0;

    for selection in &selection_set.node.items {
        match &selection.node {
            Selection::Field(_) => {
                count += 1;
            },
            Selection::FragmentSpread(fragment_spread) => {
                // Count fields in fragment spreads
                if let Some(fragment) = ctx.fragment(&fragment_spread.node.fragment_name.node) {
                    count += count_fields(&fragment.node.selection_set, ctx);
                }
            },
            Selection::InlineFragment(inline_fragment) => {
                // Count fields in inline fragments
                count += count_fields(&inline_fragment.node.selection_set, ctx);
            },
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn factory<'a>() -> SingleFieldSubscriptions {
        SingleFieldSubscriptions
    }

    #[test]
    fn simple_subscription() {
        expect_passes_rule!(
            factory,
            r#"
            subscription Sub {
              newMessage
            }
            "#,
        );
    }

    #[test]
    fn subscription_with_fragment() {
        expect_passes_rule!(
            factory,
            r#"
            subscription Sub {
              ...FragmentWithSingleField
            }
            
            fragment FragmentWithSingleField on Subscription {
              newMessage
            }
            "#,
        );
    }

    #[test]
    fn subscription_with_args() {
        expect_passes_rule!(
            factory,
            r#"
            subscription Sub {
              newMessage
            }
            "#,
        );
    }

    #[test]
    fn subscription_with_field_alias() {
        expect_passes_rule!(
            factory,
            r#"
            subscription Sub {
              aliasedField: newMessage
            }
            "#,
        );
    }

    #[test]
    fn subscription_with_multiple_fields() {
        expect_fails_rule!(
            factory,
            r#"
            subscription Sub {
              newMessage
              newNotification
            }
            "#,
        );
    }

    #[test]
    fn subscription_with_multiple_fields_and_fragment() {
        expect_fails_rule!(
            factory,
            r#"
            subscription Sub {
              newMessage
              ...FragmentWithField
            }
            
            fragment FragmentWithField on Subscription {
              newNotification
            }
            "#,
        );
    }

    #[test]
    fn non_subscription_with_multiple_fields() {
        expect_passes_rule!(
            factory,
            r#"
            query Query {
              field1
              field2
            }
            "#,
        );
    }

    #[test]
    fn mutation_with_multiple_fields() {
        expect_passes_rule!(
            factory,
            r#"
            mutation Mutation {
              field1
              field2
            }
            "#,
        );
    }
}
