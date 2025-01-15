use std::env::temp_dir;

use mongodb_agent_common::mongodb::MockDatabaseTrait;

use crate::{update, Context, UpdateArgs};

#[tokio::test]
async fn validator_object_with_no_properties_becomes_extended_json_object() -> anyhow::Result<()> {
    let mut db = MockDatabaseTrait::new();
    let context_dir = temp_dir();

    let context = Context {
        path: context_dir,
        connection_uri: None,
        display_color: false,
    };

    let args = UpdateArgs {
        sample_size: Some(100),
        no_validator_schema: None,
        all_schema_nullable: Some(false),
    };

    update(&context, &args, &db).await?;
    Ok(())
}
