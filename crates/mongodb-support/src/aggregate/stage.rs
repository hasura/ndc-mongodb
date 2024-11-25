use std::collections::BTreeMap;

use mongodb::bson::{self, Bson};
use serde::{Deserialize, Serialize};

use super::{Accumulator, Pipeline, Selection, SortDocument};

/// Aggergation Pipeline Stage. This is a work-in-progress - we are adding enum variants to match
/// MongoDB pipeline stage types as we need them in this app. For documentation on all stage types
/// see,
/// https://www.mongodb.com/docs/manual/reference/operator/aggregation-pipeline/#std-label-aggregation-pipeline-operator-reference
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum Stage {
    /// Adds new fields to documents. $addFields outputs documents that contain all existing fields
    /// from the input documents and newly added fields.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/addFields/
    #[serde(rename = "$addFields")]
    AddFields(bson::Document),

    /// Returns literal documents from input expressions.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/documents/#mongodb-pipeline-pipe.-documents
    #[serde(rename = "$documents")]
    Documents(Vec<bson::Document>),

    /// Filters the document stream to allow only matching documents to pass unmodified into the
    /// next pipeline stage. [`$match`] uses standard MongoDB queries. For each input document,
    /// outputs either one document (a match) or zero documents (no match).
    ///
    /// TODO: Create a QueryFilter type to use for Match and for the first argument to
    /// mongodb::Collection::find.
    ///
    /// [`$match`]: https://www.mongodb.com/docs/manual/reference/operator/aggregation/match/#mongodb-pipeline-pipe.-match
    #[serde(rename = "$match")]
    Match(bson::Document),

    /// Reorders the document stream by a specified sort key. Only the order changes; the documents
    /// remain unmodified. For each input document, outputs one document.
    ///
    /// TODO: Create a type for sorting
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/sort/#mongodb-pipeline-pipe.-sort
    #[serde(rename = "$sort")]
    Sort(SortDocument),

    /// Passes the first n documents unmodified to the pipeline where n is the specified limit. For
    /// each input document, outputs either one document (for the first n documents) or zero
    /// documents (after the first n documents).
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/limit/#mongodb-pipeline-pipe.-limit
    #[serde(rename = "$limit")]
    Limit(Bson),

    /// Performs a left outer join to another collection in the same database to filter in
    /// documents from the "joined" collection for processing.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/lookup/#mongodb-pipeline-pipe.-lookup
    #[serde(rename = "$lookup", rename_all = "camelCase")]
    Lookup {
        /// Specifies the foreign collection in the same database to join to the local collection.
        ///
        /// from is optional, you can use a $documents stage in a $lookup stage instead. For an
        /// example, see Use a $documents Stage in a $lookup Stage.
        #[serde(skip_serializing_if = "Option::is_none")]
        from: Option<String>,
        /// Specifies the local documents' localField to perform an equality match with the foreign
        /// documents' foreignField.
        ///
        /// If a local document does not contain a localField value, the $lookup uses a null value
        /// for the match.
        ///
        /// Must be a string. Does not begin with a dollar sign. May contain dots to select nested
        /// fields.
        #[serde(skip_serializing_if = "Option::is_none")]
        local_field: Option<String>,
        /// Specifies the foreign documents' foreignField to perform an equality match with the
        /// local documents' localField.
        ///
        /// If a foreign document does not contain a foreignField value, the $lookup uses a null
        /// value for the match.
        ///
        /// Must be a string. Does not begin with a dollar sign. May contain dots to select nested
        /// fields.
        #[serde(skip_serializing_if = "Option::is_none")]
        foreign_field: Option<String>,
        /// Optional. Specifies the variables to use in the pipeline stages. Use the variable
        /// expressions to access the document fields that are input to the pipeline.
        #[serde(rename = "let", skip_serializing_if = "Option::is_none")]
        r#let: Option<bson::Document>,
        /// Specifies the pipeline to run on the foreign collection. The pipeline returns documents
        /// from the foreign collection. To return all documents, specify an empty pipeline [].
        ///
        /// The pipeline cannot include the $out or $merge stages. Starting in v6.0, the pipeline
        /// can contain the Atlas Search $search stage as the first stage inside the pipeline. To
        /// learn more, see Atlas Search Support.
        ///
        /// The pipeline cannot directly access the document fields. Instead, define variables for
        /// the document fields using the let option and then reference the variables in the
        /// pipeline stages.
        #[serde(skip_serializing_if = "Option::is_none")]
        pipeline: Option<Pipeline>,
        /// Specifies the name of the new array field to add to the foreign documents. The new
        /// array field contains the matching documents from the foreign collection. If the
        /// specified name already exists in the foreign document, the existing field is
        /// overwritten.
        #[serde(rename = "as")]
        r#as: String,
    },

    /// Skips the first n documents where n is the specified skip number and passes the remaining
    /// documents unmodified to the pipeline. For each input document, outputs either zero
    /// documents (for the first n documents) or one document (if after the first n documents).
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/skip/#mongodb-pipeline-pipe.-skip
    #[serde(rename = "$skip")]
    Skip(Bson),

    /// Groups input documents by a specified identifier expression and applies the accumulator
    /// expression(s), if specified, to each group. Consumes all input documents and outputs one
    /// document per each distinct group. The output documents only contain the identifier field
    /// and, if specified, accumulated fields.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/group/#mongodb-pipeline-pipe.-group
    #[serde(rename = "$group")]
    Group {
        /// This is the value for the group `_id` field
        #[serde(rename = "_id")]
        key_expression: bson::Bson,

        /// Keys will appear as field names in output documents. Values for those fields will be
        /// the result of the given accumulator operation applied to each group of input documents.
        #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
        accumulators: BTreeMap<String, Accumulator>,
    },

    /// Processes multiple aggregation pipelines within a single stage on the same set of input
    /// documents. Enables the creation of multi-faceted aggregations capable of characterizing
    /// data across multiple dimensions, or facets, in a single stage.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/facet/#mongodb-pipeline-pipe.-facet
    #[serde(rename = "$facet")]
    Facet(BTreeMap<String, Pipeline>),

    /// Returns a count of the number of documents at this stage of the aggregation pipeline. The
    /// given `String` is the name of a field to be written to the aggregation result document that
    /// will contain the count.
    ///
    /// Distinct from the $count aggregation accumulator.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/count/#mongodb-pipeline-pipe.-count
    #[serde(rename = "$count")]
    Count(String),

    /// Reshapes each document in the stream, such as by adding new fields or removing existing
    /// fields. For each input document, outputs one document.
    ///
    /// See also $unset for removing existing fields.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/project/#mongodb-pipeline-pipe.-project
    #[serde(rename = "$project")]
    Project(bson::Document),

    /// Replaces a document with the specified embedded document. The operation replaces all
    /// existing fields in the input document, including the _id field. Specify a document embedded
    /// in the input document to promote the embedded document to the top level.
    ///
    /// $replaceWith is an alias for $replaceRoot stage.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/replaceRoot/#mongodb-pipeline-pipe.-replaceRoot
    #[serde(rename = "$replaceRoot", rename_all = "camelCase")]
    ReplaceRoot { new_root: Selection },

    /// Replaces a document with the specified embedded document. The operation replaces all
    /// existing fields in the input document, including the _id field. Specify a document embedded
    /// in the input document to promote the embedded document to the top level.
    ///
    /// $replaceWith is an alias for $replaceRoot stage.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/replaceWith/#mongodb-pipeline-pipe.-replaceWith
    #[serde(rename = "$replaceWith")]
    ReplaceWith(Selection),

    /// Deconstructs an array field from the input documents to output a document for each element.
    /// Each output document is the input document with the value of the array field replaced by
    /// the element.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/unwind/
    #[serde(rename = "$unwind", rename_all = "camelCase")]
    Unwind {
        /// Field path to an array field. To specify a field path, prefix the field name with
        /// a dollar sign $ and enclose in quotes.
        path: String,

        /// Optional. The name of a new field to hold the array index of the element. The name
        /// cannot start with a dollar sign $.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        include_array_index: Option<String>,

        /// Optional.
        ///
        /// - If true, if the path is null, missing, or an empty array, $unwind outputs the document.
        /// - If false, if path is null, missing, or an empty array, $unwind does not output a document.
        ///
        /// The default value is false.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preserve_null_and_empty_arrays: Option<bool>,
    },

    /// For cases where we receive pipeline stages from an external source, such as a native query,
    /// and we don't want to attempt to parse it we store the stage BSON document unaltered.
    #[serde(untagged)]
    Other(bson::Document),
}
