use std::collections::BTreeMap;

use ndc_models::{Expression, PathElement, RelationshipArgument};

#[derive(Clone, Debug)]
pub struct PathElementBuilder {
    relationship: ndc_models::RelationshipName,
    arguments: Option<BTreeMap<ndc_models::ArgumentName, RelationshipArgument>>,
    predicate: Option<Box<Expression>>,
}

pub fn path_element(relationship: ndc_models::RelationshipName) -> PathElementBuilder {
    PathElementBuilder::new(relationship)
}

impl PathElementBuilder {
    pub fn new(relationship: ndc_models::RelationshipName) -> Self {
        PathElementBuilder {
            relationship: relationship,
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
