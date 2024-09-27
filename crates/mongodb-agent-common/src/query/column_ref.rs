// Some of the methods here have been added to support future work - suppressing the dead code
// check prevents warnings in the meantime.
#![allow(dead_code)]

use std::{borrow::Cow, iter::once};

use mongodb::bson::{doc, Bson};
use ndc_query_plan::Scope;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{ComparisonTarget, OrderByTarget},
    mongodb::sanitize::is_name_safe,
};

/// Reference to a document field, or a nested property of a document field. There are two contexts
/// where we reference columns:
///
/// - match queries, where the reference is a key in the document used in a `$match` aggregation stage
/// - aggregation expressions which appear in a number of contexts
///
/// Those two contexts are not compatible. For example in aggregation expressions column names are
/// prefixed with a dollar sign ($), but in match queries names are not prefixed. Expressions may
/// reference variables, while match queries may not. Some [ComparisonTarget] values **cannot** be
/// expressed in match queries. Those include root collection column references (which require
/// a variable reference), and columns with names that include characters that MongoDB evaluates
/// specially, such as dollar signs or dots.
///
/// This type provides a helper that attempts to produce a match query reference for
/// a [ComparisonTarget], but falls back to an aggregation expression if necessary. It is up to the
/// caller to switch contexts in the second case.
#[derive(Clone, Debug, PartialEq)]
pub enum ColumnRef<'a> {
    /// Reference that can be used as a key in a match document. For example, "$imdb.rating".
    MatchKey(Cow<'a, str>),

    /// Just like MatchKey, except that this form can reference variables. For example,
    /// "$$this.title". Can only be used in aggregation expressions, is not used as a key.
    ExpressionStringShorthand(Cow<'a, str>),

    Expression(Bson),
}

impl<'a> ColumnRef<'a> {
    /// Given a column target returns a string that can be used in a MongoDB match query that
    /// references the corresponding field, either in the target collection of a query request, or
    /// in the related collection. Resolves nested fields and root collection references, but does
    /// not traverse relationships.
    ///
    /// If the given target cannot be represented as a match query key, falls back to providing an
    /// aggregation expression referencing the column.
    pub fn from_comparison_target(column: &ComparisonTarget) -> ColumnRef<'_> {
        from_comparison_target(column)
    }

    /// TODO: This will hopefully become infallible once MDB-150 & MDB-151 are implemented.
    pub fn from_order_by_target(target: &OrderByTarget) -> Result<ColumnRef<'_>, MongoAgentError> {
        from_order_by_target(target)
    }

    pub fn from_field_path<'b>(
        field_path: impl IntoIterator<Item = &'b ndc_models::FieldName>,
    ) -> ColumnRef<'b> {
        from_path(
            None,
            field_path
                .into_iter()
                .map(|field_name| field_name.as_ref() as &str),
        )
        .unwrap()
    }

    pub fn from_field(field_name: &ndc_models::FieldName) -> ColumnRef<'_> {
        fold_path_element(None, field_name.as_ref())
    }

    /// Get a reference to a pipeline variable
    pub fn variable(variable_name: impl std::fmt::Display) -> Self {
        Self::ExpressionStringShorthand(format!("$${variable_name}").into())
    }

    pub fn into_nested_field<'b: 'a>(self, field_name: &'b ndc_models::FieldName) -> ColumnRef<'b> {
        fold_path_element(Some(self), field_name.as_ref())
    }

    pub fn into_aggregate_expression(self) -> Bson {
        match self {
            ColumnRef::MatchKey(key) => format!("${key}").into(),
            ColumnRef::ExpressionStringShorthand(key) => key.to_string().into(),
            ColumnRef::Expression(expr) => expr,
        }
    }
}

fn from_comparison_target(column: &ComparisonTarget) -> ColumnRef<'_> {
    match column {
        // We exclude `path` (the relationship path) from the resulting ColumnRef because MongoDB
        // field references are not relationship-aware. Traversing relationship references is
        // handled upstream.
        ComparisonTarget::Column {
            name, field_path, ..
        } => {
            let name_and_path = once(name.as_ref() as &str).chain(
                field_path
                    .iter()
                    .flatten()
                    .map(|field_name| field_name.as_ref() as &str),
            );
            // The None case won't come up if the input to [from_target_helper] has at least
            // one element, and we know it does because we start the iterable with `name`
            from_path(None, name_and_path).unwrap()
        }
        ComparisonTarget::ColumnInScope {
            name,
            field_path,
            scope,
            ..
        } => {
            // "$$ROOT" is not actually a valid match key, but cheating here makes the
            // implementation much simpler. This match branch produces a ColumnRef::Expression
            // in all cases.
            let init = ColumnRef::variable(name_from_scope(scope));
            from_path(
                Some(init),
                once(name.as_ref() as &str).chain(
                    field_path
                        .iter()
                        .flatten()
                        .map(|field_name| field_name.as_ref() as &str),
                ),
            )
            // The None case won't come up if the input to [from_target_helper] has at least
            // one element, and we know it does because we start the iterable with `name`
            .unwrap()
        }
    }
}

fn from_order_by_target(target: &OrderByTarget) -> Result<ColumnRef<'_>, MongoAgentError> {
    match target {
        // We exclude `path` (the relationship path) from the resulting ColumnRef because MongoDB
        // field references are not relationship-aware. Traversing relationship references is
        // handled upstream.
        OrderByTarget::Column {
            name, field_path, ..
        } => {
            let name_and_path = once(name.as_ref() as &str).chain(
                field_path
                    .iter()
                    .flatten()
                    .map(|field_name| field_name.as_ref() as &str),
            );
            // The None case won't come up if the input to [from_target_helper] has at least
            // one element, and we know it does because we start the iterable with `name`
            Ok(from_path(None, name_and_path).unwrap())
        }
        OrderByTarget::SingleColumnAggregate { .. } => {
            // TODO: MDB-150
            Err(MongoAgentError::NotImplemented(
                "ordering by single column aggregate".into(),
            ))
        }
        OrderByTarget::StarCountAggregate { .. } => {
            // TODO: MDB-151
            Err(MongoAgentError::NotImplemented(
                "ordering by star count aggregate".into(),
            ))
        }
    }
}

pub fn name_from_scope(scope: &Scope) -> Cow<'_, str> {
    match scope {
        Scope::Root => "scope_root".into(),
        Scope::Named(name) => name.into(),
    }
}

fn from_path<'a>(
    init: Option<ColumnRef<'a>>,
    path: impl IntoIterator<Item = &'a str>,
) -> Option<ColumnRef<'a>> {
    path.into_iter().fold(init, |accum, element| {
        Some(fold_path_element(accum, element))
    })
}

fn fold_path_element<'a>(
    ref_so_far: Option<ColumnRef<'_>>,
    path_element: &'a str,
) -> ColumnRef<'a> {
    match (ref_so_far, is_name_safe(path_element)) {
        (Some(ColumnRef::MatchKey(parent)), true) => {
            ColumnRef::MatchKey(format!("{parent}.{path_element}").into())
        }
        (Some(ColumnRef::ExpressionStringShorthand(parent)), true) => {
            ColumnRef::ExpressionStringShorthand(format!("{parent}.{path_element}").into())
        }
        (Some(parent), _) => ColumnRef::Expression(
            doc! {
                "$getField": {
                    "input": parent.into_aggregate_expression(),
                    "field": { "$literal": path_element },
                }
            }
            .into(),
        ),
        (None, true) => ColumnRef::MatchKey(path_element.into()),
        (None, false) => ColumnRef::Expression(
            doc! {
                "$getField": {
                    "$literal": path_element
                }
            }
            .into(),
        ),
    }
}

/// Produces an aggregation expression that evaluates to the value of a given document field.
/// Unlike `column_ref` this expression cannot be used as a match query key - it can only be used
/// as an expression.
pub fn column_expression(column: &ComparisonTarget) -> Bson {
    ColumnRef::from_comparison_target(column).into_aggregate_expression()
}

#[cfg(test)]
mod tests {
    use configuration::MongoScalarType;
    use mongodb::bson::doc;
    use mongodb_support::BsonScalarType;
    use ndc_query_plan::Scope;
    use pretty_assertions::assert_eq;

    use crate::mongo_query_plan::{ComparisonTarget, Type};

    use super::ColumnRef;

    #[test]
    fn produces_match_query_key() -> anyhow::Result<()> {
        let target = ComparisonTarget::Column {
            name: "imdb".into(),
            field_path: Some(vec!["rating".into()]),
            field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::Double)),
            path: Default::default(),
        };
        let actual = ColumnRef::from_comparison_target(&target);
        let expected = ColumnRef::MatchKey("imdb.rating".into());
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn escapes_nested_field_name_with_dots() -> anyhow::Result<()> {
        let target = ComparisonTarget::Column {
            name: "subtitles".into(),
            field_path: Some(vec!["english.us".into()]),
            field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            path: Default::default(),
        };
        let actual = ColumnRef::from_comparison_target(&target);
        let expected = ColumnRef::Expression(
            doc! {
                "$getField": {
                    "input": "$subtitles",
                    "field": { "$literal": "english.us" } ,
                }
            }
            .into(),
        );
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn escapes_top_level_field_name_with_dots() -> anyhow::Result<()> {
        let target = ComparisonTarget::Column {
            name: "meta.subtitles".into(),
            field_path: Some(vec!["english_us".into()]),
            field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            path: Default::default(),
        };
        let actual = ColumnRef::from_comparison_target(&target);
        let expected = ColumnRef::Expression(
            doc! {
                "$getField": {
                    "input": { "$getField": { "$literal": "meta.subtitles" } },
                    "field": { "$literal": "english_us" },
                }
            }
            .into(),
        );
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn escapes_multiple_unsafe_nested_field_names() -> anyhow::Result<()> {
        let target = ComparisonTarget::Column {
            name: "meta".into(),
            field_path: Some(vec!["$unsafe".into(), "$also_unsafe".into()]),
            field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            path: Default::default(),
        };
        let actual = ColumnRef::from_comparison_target(&target);
        let expected = ColumnRef::Expression(
            doc! {
                "$getField": {
                    "input": {
                        "$getField": {
                            "input": "$meta",
                            "field": { "$literal": "$unsafe" },
                        }
                    },
                    "field": { "$literal": "$also_unsafe" },
                }
            }
            .into(),
        );
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn traverses_multiple_field_names_before_escaping() -> anyhow::Result<()> {
        let target = ComparisonTarget::Column {
            name: "valid_key".into(),
            field_path: Some(vec!["also_valid".into(), "$not_valid".into()]),
            field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            path: Default::default(),
        };
        let actual = ColumnRef::from_comparison_target(&target);
        let expected = ColumnRef::Expression(
            doc! {
                "$getField": {
                    "input": "$valid_key.also_valid",
                    "field": { "$literal": "$not_valid" },
                }
            }
            .into(),
        );
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn produces_dot_separated_root_column_reference() -> anyhow::Result<()> {
        let target = ComparisonTarget::ColumnInScope {
            name: "field".into(),
            field_path: Some(vec!["prop1".into(), "prop2".into()]),
            field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            scope: Scope::Root,
        };
        let actual = ColumnRef::from_comparison_target(&target);
        let expected = ColumnRef::ExpressionStringShorthand("$$scope_root.field.prop1.prop2".into());
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn escapes_unsafe_field_name_in_root_column_reference() -> anyhow::Result<()> {
        let target = ComparisonTarget::ColumnInScope {
            name: "$field".into(),
            field_path: Default::default(),
            field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            scope: Scope::Named("scope_0".into()),
        };
        let actual = ColumnRef::from_comparison_target(&target);
        let expected = ColumnRef::Expression(
            doc! {
                "$getField": {
                    "input": "$$scope_0",
                    "field": { "$literal": "$field" },
                }
            }
            .into(),
        );
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn escapes_unsafe_nested_property_name_in_root_column_reference() -> anyhow::Result<()> {
        let target = ComparisonTarget::ColumnInScope {
            name: "field".into(),
            field_path: Some(vec!["$unsafe_name".into()]),
            field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            scope: Scope::Root,
        };
        let actual = ColumnRef::from_comparison_target(&target);
        let expected = ColumnRef::Expression(
            doc! {
                "$getField": {
                    "input": "$$scope_root.field",
                    "field": { "$literal": "$unsafe_name" },
                }
            }
            .into(),
        );
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn escapes_multiple_layers_of_nested_property_names_in_root_column_reference(
    ) -> anyhow::Result<()> {
        let target = ComparisonTarget::ColumnInScope {
            name: "$field".into(),
            field_path: Some(vec!["$unsafe_name1".into(), "$unsafe_name2".into()]),
            field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            scope: Scope::Root,
        };
        let actual = ColumnRef::from_comparison_target(&target);
        let expected = ColumnRef::Expression(
            doc! {
                "$getField": {
                    "input": {
                        "$getField": {
                            "input": {
                                "$getField": {
                                    "input": "$$scope_root",
                                    "field": { "$literal": "$field" },
                                }
                            },
                            "field": { "$literal": "$unsafe_name1" },
                        }
                    },
                    "field": { "$literal": "$unsafe_name2" },
                }
            }
            .into(),
        );
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn escapes_unsafe_deeply_nested_property_name_in_root_column_reference() -> anyhow::Result<()> {
        let target = ComparisonTarget::ColumnInScope {
            name: "field".into(),
            field_path: Some(vec!["prop1".into(), "$unsafe_name".into()]),
            field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            scope: Scope::Root,
        };
        let actual = ColumnRef::from_comparison_target(&target);
        let expected = ColumnRef::Expression(
            doc! {
                "$getField": {
                    "input": "$$scope_root.field.prop1",
                    "field": { "$literal": "$unsafe_name" },
                }
            }
            .into(),
        );
        assert_eq!(actual, expected);
        Ok(())
    }
}
