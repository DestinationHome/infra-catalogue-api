use std::ops::RangeInclusive;

use async_graphql::{ComplexObject, Context, Result as GraphQLResult, SimpleObject};
use bson::doc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{api::Database, object::Object, user::User};

pub const COLLECTION_SIZE: RangeInclusive<usize> = 2..=50;

#[derive(Serialize, Deserialize, SimpleObject)]
#[graphql(complex)]
pub struct Collection {
    pub uuid: String,

    pub name: String,
    pub description: Option<String>,
    pub image_url: Option<String>,

    #[graphql(skip)]
    pub author: String,

    #[graphql(skip)]
    pub objects: Vec<String>,

    #[graphql(skip)]
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[graphql(skip)]
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[ComplexObject]
impl Collection {
    pub async fn created_at(&self) -> String {
        self.created_at.to_rfc3339()
    }
    pub async fn updated_at(&self) -> String {
        self.updated_at.to_rfc3339()
    }

    pub async fn author(&self, ctx: &Context<'_>) -> GraphQLResult<User> {
        let database = ctx.data::<Database>().unwrap();
        User::resolve(database, &self.author)
            .await
            .map_or_else(|| Err("Author not found".into()), Ok)
    }

    pub async fn objects(&self, ctx: &Context<'_>) -> Vec<Object> {
        let database = ctx.data::<Database>().unwrap();
        Object::resolve_many(database, &self.objects).await
    }
}
