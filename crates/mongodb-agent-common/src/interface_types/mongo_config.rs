use mongodb::Client;

#[derive(Clone, Debug)]
pub struct MongoConfig {
    pub client: Client,

    /// Name of the database to connect to
    pub database: String,
}
