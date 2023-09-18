use mongodb::Client;
use serde::Deserialize;

use super::object::Object;

#[derive(Clone)]
pub struct Database {
    pub client: Client,
    pub database: mongodb::Database,
    pub collection: mongodb::Collection<Object>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub page: Option<u32>,
}