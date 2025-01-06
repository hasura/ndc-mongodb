use std::collections::BTreeMap;

use ndc_models::{Expression, FieldName, PathElement, RelationshipArgument};

#[derive(Clone, Debug)]
pub struct PathElementBuilder {
    relationship: ndc_models::RelationshipName,
    arguments: Option<BTreeMap<ndc_models::ArgumentName, RelationshipArgument>>,
    field_path: Option<Vec<FieldName>>,
    predicate: Option<Box<Expression>>,
}

pub fn path_element(relationship: ndc_models::RelationshipName) -> PathElementBuilder {
    PathElementBuilder::new(relationship)
}

impl PathElementBuilder {
    pub fn new(relationship: ndc_models::RelationshipName) -> Self {
        PathElementBuilder {
            relationship,
            arguments: None,
            field_path: None,
            predicate: None,
        }
    }

    pub fn predicate(mut self, expression: Expression) -> Self {
        self.predicate = Some(Box::new(expression));
        self
    }

    pub fn field_path(
        mut self,
        field_path: impl IntoIterator<Item = impl Into<FieldName>>,
    ) -> Self {
        self.field_path = Some(field_path.into_iter().map(Into::into).collect());
        self
    }
}

impl From<PathElementBuilder> for PathElement {
    fn from(value: PathElementBuilder) -> Self {
        PathElement {
            relationship: value.relationship,
            arguments: value.arguments.unwrap_or_default(),
            field_path: value.field_path,
            predicate: value.predicate,
        }
    }
}
