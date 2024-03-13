use configuration::{metadata::{Collection, ObjectField, ObjectType}, Configuration};

pub fn v2_schema_response_to_configuration(
    response: dc_api_types::SchemaResponse,
) -> Configuration {
    let metadata = v2_schema_response_to_metadata(response);
    Configuration { metadata }
}

fn v2_schema_response_to_metadata(
    response: dc_api_types::SchemaResponse,
) -> configuration::Metadata {
    let table_object_types = response.tables.iter().map(table_to_object_type);
    let nested_object_types =
        response
            .object_types
            .into_iter()
            .map(|ot| ObjectType {
                name: ot.name.to_string(),
                description: ot.description,
                fields: ot
                    .columns
                    .into_iter()
                    .map(column_info_to_object_field)
                    .collect(),
            });
    let object_types = table_object_types.chain(nested_object_types).collect();

    let collections = response
        .tables
        .into_iter()
        .map(|table| table_to_collection(table))
        .collect();

    configuration::Metadata {
        collections,
        object_types,
    }
}

fn column_info_to_object_field(column_info: dc_api_types::ColumnInfo) -> ObjectField {
    let t = v2_to_v3_column_type(column_info.r#type);
    let is_nullable = column_info.nullable;
        ObjectField {
            name: column_info.name,
            description: column_info.description.flatten(),
            r#type: maybe_nullable(t, is_nullable),
        }
}

fn maybe_nullable(t: configuration::metadata::Type, is_nullable: bool) -> configuration::metadata::Type {
    todo!()
}

fn v2_to_v3_column_type(r#type: dc_api_types::ColumnType) -> configuration::metadata::Type {
    todo!()
}

fn table_to_object_type(table: &dc_api_types::TableInfo) -> ObjectType {
    let fields = table
        .columns
        .iter()
        .map(|column_info| column_info_to_object_field(column_info.clone()))
        .collect();
    ObjectType {
        name: collection_type_name_from_table_name(table.name.clone()),
        description: table.description.clone().flatten(),
        fields,
    }
}

fn collection_type_name_from_table_name(clone: Vec<String>) -> String {
    todo!()
}

fn table_to_collection(
    table: dc_api_types::TableInfo,
) -> Collection {
    let collection_type = collection_type_name_from_table_name(table.name.clone());
    Collection {
        name: name_from_qualified_name(table.name),
        description: table.description.flatten(),
        r#type: todo!(),
        }
}

fn name_from_qualified_name(name: Vec<String>) -> String {
    todo!()
}
