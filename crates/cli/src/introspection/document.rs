use configuration::{
    schema::{Collection, ObjectField, ObjectType, Type},
    Schema,
};
use mongodb::bson::{Bson, Document};
use mongodb_agent_common::interface_types::{MongoAgentError, MongoConfig};
use mongodb_support::{BsonScalarType, BsonScalarType::*, BsonType};

pub fn schema_from_document(collection_name: &str, document: &Document) -> Schema {
    let (object_types, collection) = make_collection(collection_name, document);
    Schema {
        collections: vec![collection],
        object_types,
    }
}

fn make_collection(collection_name: &str, document: &Document) -> (Vec<ObjectType>, Collection) {
    let object_type_defs = make_object_type(collection_name, document);
    let collection_info = Collection {
        name: collection_name.to_string(),
        description: None,
        r#type: collection_name.to_string(),
    };

    (object_type_defs, collection_info)
}

fn make_object_type(object_type_name: &str, document: &Document) -> Vec<ObjectType> {
    let (mut object_type_defs, object_fields) = {
        let type_prefix = format!("{object_type_name}_");
        let (object_type_defs, object_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) = document
            .iter()
            .map(|(field_name, field_value)| make_object_fields(&type_prefix, field_name, field_value))
            .unzip();
        (object_type_defs.concat(), object_fields)
    };

    let object_type = ObjectType {
        name: object_type_name.to_string(),
        description: None,
        fields: object_fields,
    };

    object_type_defs.push(object_type);
    object_type_defs
}

fn make_object_fields(
    type_prefix: &str,
    field_name: &str,
    field_value: &Bson,
) -> (Vec<ObjectType>, ObjectField) {
    let object_type_name = format!("{type_prefix}{field_name}");
    let (collected_otds, field_type) = make_field_type(&object_type_name, field_value);

    let object_field = ObjectField {
        name: field_name.to_owned(),
        description: None,
        r#type: Type::Nullable((Box::new(field_type))),
    };

    (collected_otds, object_field)
}

fn make_field_type(object_type_name: &str, field_value: &Bson) -> (Vec<ObjectType>, Type) {
    fn scalar(t: BsonScalarType) -> (Vec<ObjectType>, Type) {
        (vec![], Type::Scalar(t))
    }
    match field_value {
        Bson::Double(_) => scalar(Double),
        Bson::String(_) => scalar(String),
        Bson::Array(arr) => {
            // TODO: examine all elements of the array and take the union.
            let (collected_otds, element_type) = match arr.first() {
                Some(elem) => make_field_type(object_type_name, elem),
                None => scalar(Undefined),
            };
            (collected_otds, Type::ArrayOf(Box::new(element_type)))
        }
        Bson::Document(document) => {
            let collected_otds = make_object_type(object_type_name, document);
            (collected_otds, Type::Object(object_type_name.to_owned()))
        }
        Bson::Boolean(_) => scalar(Bool),
        Bson::Null => scalar(Null),
        Bson::RegularExpression(_) => scalar(Regex),
        Bson::JavaScriptCode(_) => scalar(Javascript),
        Bson::JavaScriptCodeWithScope(_) => scalar(JavascriptWithScope),
        Bson::Int32(_) => scalar(Int),
        Bson::Int64(_) => scalar(Long),
        Bson::Timestamp(_) => scalar(Timestamp),
        Bson::Binary(_) => scalar(BinData),
        Bson::ObjectId(_) => scalar(ObjectId),
        Bson::DateTime(_) => scalar(Date),
        Bson::Symbol(_) => scalar(Symbol),
        Bson::Decimal128(_) => scalar(Decimal),
        Bson::Undefined => scalar(Undefined),
        Bson::MaxKey => scalar(MaxKey),
        Bson::MinKey => scalar(MinKey),
        Bson::DbPointer(_) => scalar(DbPointer),
    }
}
