use std::collections::BTreeMap;

use ndc_models::{Expression, PathElement, RelationshipArgument};

#[derive(Clone, Debug)]
pub struct PathElementBuilder {
    relationship: String,
    arguments: Option<BTreeMap<String, RelationshipArgument>>,
    predicate: Option<Box<Expression>>,
}

pub fn path_element(relationship: &str) -> PathElementBuilder {
    PathElementBuilder::new(relationship)
}

impl PathElementBuilder {
    pub fn new(relationship: &str) -> Self {
        PathElementBuilder {
            relationship: relationship.to_owned(),
            arguments: None,
            predicate: None,
        }
    }

    pub fn predicate(mut self, expression: Expression) -> Self {
        self.predicate = Some(Box::new(expression));
        self
    }
}

impl From<PathElementBuilder> for PathElement {
    fn from(value: PathElementBuilder) -> Self {
        PathElement {
            relationship: value.relationship,
            arguments: value.arguments.unwrap_or_default(),
            predicate: value.predicate,
        }
    }
}
