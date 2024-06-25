use http::StatusCode;
use mongodb::bson::{doc, Document};

use crate::{interface_types::MongoAgentError, state::ConnectorState};

pub async fn check_health(state: &ConnectorState) -> Result<StatusCode, MongoAgentError> {
    let db = state.database();

    let status: Result<Document, _> = db.run_command(doc! { "ping": 1 }, None).await;

    match status {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Ok(StatusCode::SERVICE_UNAVAILABLE),
    }
}
