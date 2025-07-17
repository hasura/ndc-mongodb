use std::{collections::BTreeMap, path::Path};

use anyhow::{anyhow, ensure};
use itertools::Itertools;
use mongodb_support::ExtendedJsonMode;
use ndc_models as ndc;
use serde::{Deserialize, Serialize};

use crate::{
    native_mutation::NativeMutation,
    native_query::{NativeQuery, NativeQueryRepresentation},
    read_directory, schema, serialized,
};

#[derive(Clone, Debug, Default)]
pub struct Configuration {
    /// Tracked collections from the configured MongoDB database. This includes real collections as
    /// well as virtual collections defined by native queries using
    /// [NativeQueryRepresentation::Collection] representation.
    pub collections: BTreeMap<ndc::CollectionName, ndc::CollectionInfo>,

    /// Functions are based on native queries using [NativeQueryRepresentation::Function]
    /// representation.
    ///
    /// In query requests functions and collections are treated as the same, but in schema
    /// responses they are separate concepts. So we want a set of [CollectionInfo] values for
    /// functions for query processing, and we want it separate from `collections` for the schema
    /// response.
    pub functions: BTreeMap<ndc::FunctionName, (ndc::FunctionInfo, ndc::CollectionInfo)>,

    /// Procedures are based on native mutations.
    pub procedures: BTreeMap<ndc::ProcedureName, ndc::ProcedureInfo>,

    /// Native mutations allow arbitrary MongoDB commands where types of results are specified via
    /// user configuration.
    pub native_mutations: BTreeMap<ndc::ProcedureName, NativeMutation>,

    /// Native queries allow arbitrary aggregation pipelines that can be included in a query plan.
    pub native_queries: BTreeMap<ndc::FunctionName, NativeQuery>,

    /// Object types defined for this connector include types of documents in each collection,
    /// types for objects inside collection documents, types for native query and native mutation
    /// arguments and results.
    ///
    /// The object types here combine object type defined in files in the `schema/`,
    /// `native_queries/`, and `native_mutations/` subdirectories in the connector configuration
    /// directory.
    pub object_types: BTreeMap<ndc::ObjectTypeName, ndc::ObjectType>,

    pub options: ConfigurationOptions,
}

impl Configuration {
    pub fn validate(
        schema: serialized::Schema,
        native_mutations: BTreeMap<ndc::ProcedureName, serialized::NativeMutation>,
        native_queries: BTreeMap<ndc::FunctionName, serialized::NativeQuery>,
        options: ConfigurationOptions,
    ) -> anyhow::Result<Self> {
        tracing::debug!(
            schema = %serde_json::to_string(&schema).unwrap(),
            ?native_mutations,
            ?native_queries,
            options = %serde_json::to_string(&options).unwrap(),
            "parsing connector configuration"
        );

        let object_types_iter = || merge_object_types(&schema, &native_mutations, &native_queries);
        let object_type_errors = {
            let duplicate_type_names: Vec<&ndc::TypeName> = object_types_iter()
                .map(|(name, _)| name.as_ref())
                .duplicates()
                .collect();
            if duplicate_type_names.is_empty() {
                None
            } else {
                Some(anyhow!(
                    "configuration contains multiple definitions for these object type names: {}",
                    duplicate_type_names
                        .into_iter()
                        .map(|tn| tn.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        };
        let object_types = object_types_iter()
            .map(|(name, ot)| (name.to_owned(), ot.clone()))
            .collect();

        let collections = {
            let regular_collections = schema.collections.into_iter().map(|(name, collection)| {
                (
                    name.clone(),
                    collection_to_collection_info(&object_types, name, collection),
                )
            });
            let native_query_collections = native_queries.iter().filter_map(
                |(name, native_query): (&ndc::FunctionName, &serialized::NativeQuery)| {
                    if native_query.representation == NativeQueryRepresentation::Collection {
                        Some((
                            name.as_ref().to_owned(),
                            native_query_to_collection_info(&object_types, name, native_query),
                        ))
                    } else {
                        None
                    }
                },
            );
            regular_collections
                .chain(native_query_collections)
                .collect()
        };

        let (functions, function_errors): (BTreeMap<_, _>, Vec<_>) = native_queries
            .iter()
            .filter_map(|(name, native_query)| {
                if native_query.representation == NativeQueryRepresentation::Function {
                    Some((
                        name,
                        native_query_to_function_info(&object_types, name, native_query),
                        native_query_to_collection_info(&object_types, name, native_query),
                    ))
                } else {
                    None
                }
            })
            .map(|(name, function_result, collection_info)| {
                Ok((name.to_owned(), (function_result?, collection_info)))
                    as Result<_, anyhow::Error>
            })
            .partition_result();

        let procedures = native_mutations
            .iter()
            .map(|(name, native_mutation)| {
                (
                    name.to_owned(),
                    native_mutation_to_procedure_info(name, native_mutation),
                )
            })
            .collect();

        let ndc_object_types = object_types
            .into_iter()
            .map(|(name, ot)| (name, ot.into()))
            .collect();

        let internal_native_queries: BTreeMap<_, _> = native_queries
            .into_iter()
            .map(|(name, nq)| {
                Ok((name, NativeQuery::from_serialized(&ndc_object_types, nq)?))
                    as Result<_, anyhow::Error>
            })
            .try_collect()?;

        let internal_native_mutations: BTreeMap<_, _> = native_mutations
            .into_iter()
            .map(|(name, np)| {
                Ok((
                    name,
                    NativeMutation::from_serialized(&ndc_object_types, np)?,
                )) as Result<_, anyhow::Error>
            })
            .try_collect()?;

        let errors: Vec<String> = object_type_errors
            .into_iter()
            .chain(function_errors)
            .map(|e| e.to_string())
            .collect();
        ensure!(
            errors.is_empty(),
            "connector configuration has errrors:\n  - {}",
            errors.join("\n  - ")
        );

        Ok(Configuration {
            collections,
            functions,
            procedures,
            native_mutations: internal_native_mutations,
            native_queries: internal_native_queries,
            object_types: ndc_object_types,
            options,
        })
    }

    pub fn from_schema(schema: serialized::Schema) -> anyhow::Result<Self> {
        Self::validate(
            schema,
            Default::default(),
            Default::default(),
            Default::default(),
        )
    }

    pub async fn parse_configuration(
        configuration_dir: impl AsRef<Path> + Send,
    ) -> anyhow::Result<Self> {
        read_directory(configuration_dir).await
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationOptions {
    /// Options for introspection
    pub introspection_options: ConfigurationIntrospectionOptions,

    /// Options that affect how BSON data from MongoDB is translated to JSON in GraphQL query
    /// responses.
    #[serde(default)]
    pub serialization_options: ConfigurationSerializationOptions,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationIntrospectionOptions {
    // For introspection how many documents should be sampled per collection.
    pub sample_size: u32,

    // Whether to try validator schema first if one exists.
    pub no_validator_schema: bool,

    // Default to setting all schema fields, except the _id field on collection types, as nullable.
    pub all_schema_nullable: bool,
}

impl Default for ConfigurationIntrospectionOptions {
    fn default() -> Self {
        ConfigurationIntrospectionOptions {
            sample_size: 100,
            no_validator_schema: false,
            all_schema_nullable: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationSerializationOptions {
    /// Extended JSON has two modes: canonical and relaxed. This option determines which mode is
    /// used for output. This setting has no effect on inputs (query arguments, etc.).
    #[serde(default)]
    pub extended_json_mode: ExtendedJsonMode,

    /// When sending response data the connector may encounter data in a field that does not match
    /// the type declared for that field in the connector schema. This option specifies what the
    /// connector should do in this situation.
    #[serde(default)]
    pub on_response_type_mismatch: OnResponseTypeMismatch,
}

/// Options for connector behavior on encountering a type mismatch between query response data, and
/// declared types in schema.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OnResponseTypeMismatch {
    /// On a type mismatch, send an error instead of response data. Fails the entire query.
    #[default]
    Fail,

    /// If any field in a response row contains data of an incorrect type, exclude that row from
    /// the response.
    SkipRow,
}

fn merge_object_types<'a>(
    schema: &'a serialized::Schema,
    native_mutations: &'a BTreeMap<ndc::ProcedureName, serialized::NativeMutation>,
    native_queries: &'a BTreeMap<ndc::FunctionName, serialized::NativeQuery>,
) -> impl Iterator<Item = (&'a ndc::ObjectTypeName, &'a schema::ObjectType)> {
    let object_types_from_schema = schema.object_types.iter();
    let object_types_from_native_mutations = native_mutations
        .values()
        .flat_map(|native_mutation| &native_mutation.object_types);
    let object_types_from_native_queries = native_queries
        .values()
        .flat_map(|native_query| &native_query.object_types);
    object_types_from_schema
        .chain(object_types_from_native_mutations)
        .chain(object_types_from_native_queries)
}

fn collection_to_collection_info(
    object_types: &BTreeMap<ndc::ObjectTypeName, schema::ObjectType>,
    name: ndc::CollectionName,
    collection: schema::Collection,
) -> ndc::CollectionInfo {
    let pk_constraint =
        get_primary_key_uniqueness_constraint(object_types, &name, &collection.r#type);

    ndc::CollectionInfo {
        name,
        collection_type: collection.r#type,
        description: collection.description,
        arguments: Default::default(),
        uniqueness_constraints: BTreeMap::from_iter(pk_constraint),
        relational_mutations: None,
    }
}

fn native_query_to_collection_info(
    object_types: &BTreeMap<ndc::ObjectTypeName, schema::ObjectType>,
    name: &ndc::FunctionName,
    native_query: &serialized::NativeQuery,
) -> ndc::CollectionInfo {
    let pk_constraint = get_primary_key_uniqueness_constraint(
        object_types,
        name.as_ref(),
        &native_query.result_document_type,
    );

    // TODO: recursively verify that all referenced object types exist
    ndc::CollectionInfo {
        name: name.to_owned().into(),
        collection_type: native_query.result_document_type.clone(),
        description: native_query.description.clone(),
        arguments: arguments_to_ndc_arguments(native_query.arguments.clone()),
        uniqueness_constraints: BTreeMap::from_iter(pk_constraint),
        relational_mutations: None,
    }
}

fn get_primary_key_uniqueness_constraint(
    object_types: &BTreeMap<ndc::ObjectTypeName, schema::ObjectType>,
    name: &ndc::CollectionName,
    collection_type: &ndc::ObjectTypeName,
) -> Option<(String, ndc::UniquenessConstraint)> {
    // Check to make sure our collection's object type contains the _id field
    // If it doesn't (should never happen, all collections need an _id column), don't generate the constraint
    let object_type = object_types.get(collection_type)?;
    let id_field = object_type.fields.get("_id")?;
    match &id_field.r#type {
        schema::Type::Scalar(scalar_type) if scalar_type.is_comparable() => Some(()),
        _ => None,
    }?;
    let uniqueness_constraint = ndc::UniquenessConstraint {
        unique_columns: vec!["_id".into()],
    };
    let constraint_name = format!("{}_id", name);
    Some((constraint_name, uniqueness_constraint))
}

fn native_query_to_function_info(
    object_types: &BTreeMap<ndc::ObjectTypeName, schema::ObjectType>,
    name: &ndc::FunctionName,
    native_query: &serialized::NativeQuery,
) -> anyhow::Result<ndc::FunctionInfo> {
    Ok(ndc::FunctionInfo {
        name: name.to_owned(),
        description: native_query.description.clone(),
        arguments: arguments_to_ndc_arguments(native_query.arguments.clone()),
        result_type: function_result_type(object_types, name, &native_query.result_document_type)?,
    })
}

fn function_result_type(
    object_types: &BTreeMap<ndc::ObjectTypeName, schema::ObjectType>,
    function_name: &ndc::FunctionName,
    object_type_name: &ndc::ObjectTypeName,
) -> anyhow::Result<ndc::Type> {
    let object_type = find_object_type(object_types, object_type_name)?;
    let value_field = object_type.fields.get("__value").ok_or_else(|| {
        anyhow!("the type of the native query, {function_name}, is not valid: the type of a native query that is represented as a function must be an object type with a single field named \"__value\"")

    })?;
    Ok(value_field.r#type.clone().into())
}

fn native_mutation_to_procedure_info(
    mutation_name: &ndc::ProcedureName,
    mutation: &serialized::NativeMutation,
) -> ndc::ProcedureInfo {
    ndc::ProcedureInfo {
        name: mutation_name.to_owned(),
        description: mutation.description.clone(),
        arguments: arguments_to_ndc_arguments(mutation.arguments.clone()),
        result_type: mutation.result_type.clone().into(),
    }
}

fn arguments_to_ndc_arguments(
    configured_arguments: BTreeMap<ndc::ArgumentName, schema::ObjectField>,
) -> BTreeMap<ndc::ArgumentName, ndc::ArgumentInfo> {
    configured_arguments
        .into_iter()
        .map(|(name, field)| {
            (
                name,
                ndc::ArgumentInfo {
                    argument_type: field.r#type.into(),
                    description: field.description,
                },
            )
        })
        .collect()
}

fn find_object_type<'a>(
    object_types: &'a BTreeMap<ndc::ObjectTypeName, schema::ObjectType>,
    object_type_name: &ndc::ObjectTypeName,
) -> anyhow::Result<&'a schema::ObjectType> {
    object_types
        .get(object_type_name)
        .ok_or_else(|| anyhow!("configuration references an object type named {object_type_name}, but it is not defined"))
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::*;
    use crate::{schema::Type, serialized::Schema};

    #[test]
    fn fails_with_duplicate_object_types() {
        let schema = Schema {
            collections: Default::default(),
            object_types: [(
                "Album".to_owned().into(),
                schema::ObjectType {
                    fields: Default::default(),
                    description: Default::default(),
                },
            )]
            .into_iter()
            .collect(),
        };
        let native_mutations = [(
            "hello".into(),
            serialized::NativeMutation {
                object_types: [(
                    "Album".to_owned().into(),
                    schema::ObjectType {
                        fields: Default::default(),
                        description: Default::default(),
                    },
                )]
                .into_iter()
                .collect(),
                result_type: Type::Object("Album".to_owned()),
                command: doc! { "command": 1 },
                arguments: Default::default(),
                selection_criteria: Default::default(),
                description: Default::default(),
            },
        )]
        .into_iter()
        .collect();
        let result = Configuration::validate(
            schema,
            native_mutations,
            Default::default(),
            Default::default(),
        );
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("multiple definitions"));
        assert!(error_msg.contains("Album"));
    }
}
