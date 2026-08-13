use async_graphql::SimpleObject;
use bson::doc;
use serde::{Deserialize, Serialize};

use super::api::Database;

#[derive(Clone, Debug, Serialize, Deserialize, SimpleObject)]
pub struct User {
    pub uuid: String,

    pub display_name: String,

    #[graphql(skip)]
    pub token: String,
}

impl User {
    pub async fn resolve(database: &Database, uuid: &str) -> Option<Self> {
        database
            .users
            .find_one(doc! { "uuid": uuid }, None)
            .await
            .unwrap()
    }
}
