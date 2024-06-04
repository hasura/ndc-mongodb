use core::hash::Hash;
use std::collections::BTreeMap;

use indexmap::IndexMap;
use itertools::{merge_join_by, EitherOrBoth, Itertools};
use ndc_models::RelationshipArgument;
use thiserror::Error;

use crate::{
    Aggregate, ConnectorTypes, Expression, Field, NestedArray, NestedField, NestedObject, Query,
    Relationship, Relationships,
};

#[derive(Clone, Debug, Error)]
pub enum RelationshipUnificationError {
    #[error("relationship arguments mismatch")]
    ArgumentsMismatch {
        a: BTreeMap<String, RelationshipArgument>,
        b: BTreeMap<String, RelationshipArgument>,
    },

    #[error("relationships select fields with the same name, {field_name}, but that have different types")]
    FieldTypeMismatch { field_name: String },

    #[error("relationships select columns {column_a} and {column_b} with the same field name, {field_name}")]
    FieldColumnMismatch {
        field_name: String,
        column_a: String,
        column_b: String,
    },

    #[error("relationship references have incompatible configurations: {}", .0.join(", "))]
    Mismatch(Vec<&'static str>),

    #[error("relationship references referenced different nested relationships with the same field name, {field_name}")]
    RelationshipMismatch { field_name: String },
}

type Result<T> = std::result::Result<T, RelationshipUnificationError>;

/// Given two relationships with possibly different configurations, produce a new relationship that
/// covers the needs of both inputs. For example if the two inputs have different field selections
/// then the output selects all fields of both inputs.
///
/// Returns an error if the relationships cannot be unified due to incompatibilities. For example
/// if the input relationships have different predicates or offsets then they cannot be unified.
pub fn unify_relationship_references<T>(
    a: Relationship<T>,
    b: Relationship<T>,
) -> Result<Relationship<T>>
where
    T: ConnectorTypes,
{
    let relationship = Relationship {
        column_mapping: a.column_mapping,
        relationship_type: a.relationship_type,
        target_collection: a.target_collection,
        arguments: unify_arguments(a.arguments, b.arguments)?,
        query: unify_query(a.query, b.query)?,
    };
    Ok(relationship)
}

// TODO: The engine may be set up to avoid a situation where we encounter a mismatch. For now we're
// being pessimistic, and if we get an error here we record the two relationships under separate
// keys instead of recording one, unified relationship.
fn unify_arguments(
    a: BTreeMap<String, RelationshipArgument>,
    b: BTreeMap<String, RelationshipArgument>,
) -> Result<BTreeMap<String, RelationshipArgument>> {
    if a != b {
        Err(RelationshipUnificationError::ArgumentsMismatch { a, b })
    } else {
        Ok(a)
    }
}

fn unify_query<T>(a: Query<T>, b: Query<T>) -> Result<Query<T>>
where
    T: ConnectorTypes,
{
    let predicate_a = a.predicate.and_then(simplify_expression);
    let predicate_b = b.predicate.and_then(simplify_expression);

    let mismatching_fields = [
        (a.limit != b.limit, "limit"),
        (a.aggregates_limit != b.aggregates_limit, "aggregates_limit"),
        (a.offset != b.offset, "offset"),
        (a.order_by != b.order_by, "order_by"),
        (predicate_a != predicate_b, "predicate"),
    ]
    .into_iter()
    .filter_map(|(is_mismatch, field_name)| if is_mismatch { Some(field_name) } else { None })
    .collect_vec();

    if !mismatching_fields.is_empty() {
        return Err(RelationshipUnificationError::Mismatch(mismatching_fields));
    }

    let query = Query {
        aggregates: unify_aggregates(a.aggregates, b.aggregates)?,
        fields: unify_fields(a.fields, b.fields)?,
        limit: a.limit,
        aggregates_limit: a.aggregates_limit,
        offset: a.offset,
        order_by: a.order_by,
        predicate: predicate_a,
        relationships: unify_nested_relationships(a.relationships, b.relationships)?,
    };
    Ok(query)
}

fn unify_aggregates<T>(
    a: Option<IndexMap<String, Aggregate<T>>>,
    b: Option<IndexMap<String, Aggregate<T>>>,
) -> Result<Option<IndexMap<String, Aggregate<T>>>>
where
    T: ConnectorTypes,
{
    if a != b {
        Err(RelationshipUnificationError::Mismatch(vec!["aggregates"]))
    } else {
        Ok(a)
    }
}

fn unify_fields<T>(
    a: Option<IndexMap<String, Field<T>>>,
    b: Option<IndexMap<String, Field<T>>>,
) -> Result<Option<IndexMap<String, Field<T>>>>
where
    T: ConnectorTypes,
{
    unify_options(a, b, unify_fields_some)
}

fn unify_fields_some<T>(
    fields_a: IndexMap<String, Field<T>>,
    fields_b: IndexMap<String, Field<T>>,
) -> Result<IndexMap<String, Field<T>>>
where
    T: ConnectorTypes,
{
    let fields = merged_map_values(fields_a, fields_b)
        .map(|entry| match entry {
            EitherOrBoth::Both((name, field_a), (_, field_b)) => {
                let field = unify_field(&name, field_a, field_b)?;
                Ok((name, field))
            }
            EitherOrBoth::Left((name, field_a)) => Ok((name, field_a)),
            EitherOrBoth::Right((name, field_b)) => Ok((name, field_b)),
        })
        .try_collect()?;
    Ok(fields)
}

fn unify_field<T>(field_name: &str, a: Field<T>, b: Field<T>) -> Result<Field<T>>
where
    T: ConnectorTypes,
{
    match (a, b) {
        (
            Field::Column {
                column: column_a,
                fields: nested_fields_a,
                column_type, // if columns match then column_type should also match
            },
            Field::Column {
                column: column_b,
                fields: nested_fields_b,
                ..
            },
        ) => {
            if column_a != column_b {
                Err(RelationshipUnificationError::FieldColumnMismatch {
                    field_name: field_name.to_owned(),
                    column_a,
                    column_b,
                })
            } else {
                Ok(Field::Column {
                    column: column_a,
                    column_type,
                    fields: unify_nested_fields(nested_fields_a, nested_fields_b)?,
                })
            }
        }
        (
            Field::Relationship {
                relationship: relationship_a,
                aggregates: aggregates_a,
                fields: fields_a,
            },
            Field::Relationship {
                relationship: relationship_b,
                aggregates: aggregates_b,
                fields: fields_b,
            },
        ) => {
            if relationship_a != relationship_b {
                Err(RelationshipUnificationError::RelationshipMismatch {
                    field_name: field_name.to_owned(),
                })
            } else {
                Ok(Field::Relationship {
                    relationship: relationship_b,
                    aggregates: unify_aggregates(aggregates_a, aggregates_b)?,
                    fields: unify_fields(fields_a, fields_b)?,
                })
            }
        }
        _ => Err(RelationshipUnificationError::FieldTypeMismatch {
            field_name: field_name.to_owned(),
        }),
    }
}

fn unify_nested_fields<T>(
    a: Option<NestedField<T>>,
    b: Option<NestedField<T>>,
) -> Result<Option<NestedField<T>>>
where
    T: ConnectorTypes,
{
    unify_options(a, b, unify_nested_fields_some)
}

fn unify_nested_fields_some<T>(a: NestedField<T>, b: NestedField<T>) -> Result<NestedField<T>>
where
    T: ConnectorTypes,
{
    match (a, b) {
        (
            NestedField::Object(NestedObject { fields: fields_a }),
            NestedField::Object(NestedObject { fields: fields_b }),
        ) => Ok(NestedField::Object(NestedObject {
            fields: unify_fields_some(fields_a, fields_b)?,
        })),
        (
            NestedField::Array(NestedArray { fields: nested_a }),
            NestedField::Array(NestedArray { fields: nested_b }),
        ) => Ok(NestedField::Array(NestedArray {
            fields: Box::new(unify_nested_fields_some(*nested_a, *nested_b)?),
        })),
        _ => Err(RelationshipUnificationError::Mismatch(vec!["nested field"])),
    }
}

fn unify_nested_relationships<T>(
    a: Relationships<T>,
    b: Relationships<T>,
) -> Result<Relationships<T>>
where
    T: ConnectorTypes,
{
    merged_map_values(a, b)
        .map(|entry| match entry {
            EitherOrBoth::Both((name, a), (_, b)) => {
                Ok((name, unify_relationship_references(a, b)?))
            }
            EitherOrBoth::Left((name, a)) => Ok((name, a)),
            EitherOrBoth::Right((name, b)) => Ok((name, b)),
        })
        .try_collect()
}

/// In some cases we receive the predicate expression `Some(Expression::And [])` which does not
/// filter out anything, but fails equality checks with `None`. Simplifying that expression to
/// `None` allows us to unify relationship references that we wouldn't otherwise be able to.
fn simplify_expression<T>(expr: Expression<T>) -> Option<Expression<T>>
where
    T: ConnectorTypes,
{
    match expr {
        Expression::And { expressions } if expressions.is_empty() => None,
        e => Some(e),
    }
}

fn unify_options<T>(
    a: Option<T>,
    b: Option<T>,
    unify_some: fn(a: T, b: T) -> Result<T>,
) -> Result<Option<T>> {
    let union = match (a, b) {
        (None, None) => None,
        (None, Some(b)) => Some(b),
        (Some(a), None) => Some(a),
        (Some(a), Some(b)) => Some(unify_some(a, b)?),
    };
    Ok(union)
}

/// Create an iterator over keys and values from two maps. The iterator includes on entry for the
/// union of the sets of keys from both maps, combined with optional values for that key from both
/// input maps.
fn merged_map_values<K, V1, V2>(
    map_a: impl IntoIterator<Item = (K, V1)>,
    map_b: impl IntoIterator<Item = (K, V2)>,
) -> impl Iterator<Item = EitherOrBoth<(K, V1), (K, V2)>>
where
    K: Hash + Ord + 'static,
{
    // Entries must be sorted for merge_join_by to work correctly
    let entries_a = map_a
        .into_iter()
        .sorted_unstable_by(|(key_1, _), (key_2, _)| key_1.cmp(key_2));
    let entries_b = map_b
        .into_iter()
        .sorted_unstable_by(|(key_1, _), (key_2, _)| key_1.cmp(key_2));

    merge_join_by(entries_a, entries_b, |(key_a, _), (key_b, _)| {
        key_a.cmp(key_b)
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::{
        field, object,
        plan_for_query_request::plan_test_helpers::{
            date, double, int, object_type, relationship, string, TestContext,
        },
        Relationship,
    };

    use super::unify_relationship_references;

    #[test]
    fn unifies_relationships_with_differing_fields() -> anyhow::Result<()> {
        let a: Relationship<TestContext> = relationship("movies")
            .fields([field!("title": string()), field!("year": int())])
            .into();

        let b = relationship("movies")
            .fields([field!("year": int()), field!("rated": string())])
            .into();

        let expected = relationship("movies")
            .fields([
                field!("title": string()),
                field!("year": int()),
                field!("rated": string()),
            ])
            .into();

        let unified = unify_relationship_references(a, b)?;
        assert_eq!(unified, expected);
        Ok(())
    }

    #[test]
    fn unifies_relationships_with_differing_aliases_for_field() -> anyhow::Result<()> {
        let a: Relationship<TestContext> = relationship("movies")
            .fields([field!("title": string())])
            .into();

        let b: Relationship<TestContext> = relationship("movies")
            .fields([field!("movie_title" => "title": string())])
            .into();

        let expected = relationship("movies")
            .fields([
                field!("title": string()),
                field!("movie_title" => "title": string()),
            ])
            .into();

        let unified = unify_relationship_references(a, b)?;
        assert_eq!(unified, expected);
        Ok(())
    }

    #[test]
    fn unifies_nested_field_selections() -> anyhow::Result<()> {
        let tomatoes_type = object_type([
            (
                "viewer",
                object_type([("numReviews", int()), ("rating", double())]),
            ),
            ("lastUpdated", date()),
        ]);

        let a: Relationship<TestContext> = relationship("movies")
            .fields([
                field!("tomatoes" => "tomatoes": tomatoes_type.clone(), object!([
                    field!("viewer" => "viewer": string(), object!([
                        field!("rating": double())
                    ]))
                ])),
            ])
            .into();

        let b: Relationship<TestContext> = relationship("movies")
            .fields([
                field!("tomatoes" => "tomatoes": tomatoes_type.clone(), object!([
                    field!("viewer" => "viewer": string(), object!([
                        field!("numReviews": int())
                    ])),
                    field!("lastUpdated": date())
                ])),
            ])
            .into();

        let expected: Relationship<TestContext> = relationship("movies")
            .fields([
                field!("tomatoes" => "tomatoes": tomatoes_type.clone(), object!([
                    field!("viewer" => "viewer": string(), object!([
                        field!("rating": double()),
                        field!("numReviews": int())
                    ])),
                    field!("lastUpdated": date())
                ])),
            ])
            .into();

        let unified = unify_relationship_references(a, b)?;
        assert_eq!(unified, expected);
        Ok(())
    }
}
