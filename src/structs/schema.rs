use async_graphql::{Object as GraphQLObject, Schema, EmptyMutation, EmptySubscription, Context, Result as GraphQLResult};
use bson::doc;

use super::{api::Database, object::Locale};
use crate::{Object, ODC_IGNORE};

pub struct Query;

#[GraphQLObject]
impl Query {
    async fn search(&self, ctx: &Context<'_>, query: String, locale: Option<Locale>) -> GraphQLResult<Vec<Object>> {
        let database = ctx.data::<Database>().unwrap();

        let mut cursor = database.collection.find(doc! {
            "$text": { "$search": &query }
        }, None).await.unwrap();

        let mut uuids: Vec<String> = vec![];
        let mut results: Vec<Object> = vec![];
        while let Ok(res) = cursor.advance().await {
            if !res { break; } // No more requests

            match cursor.deserialize_current() {
                Ok(mut object) => {
                    if uuids.contains(&object.uuid) { continue; }
                    if ODC_IGNORE.contains(&object.uuid) { 
                        log::info!("Object {} is set to be ignored (query: `{}`)", object.uuid, query);
                        continue;
                    }

                    // Translate the object to the requested locale
                    let document = cursor.current();
                    object.localize(document, locale);

                    uuids.push(object.uuid.clone());
                    results.push(object);
                },
                Err(e) => {
                    let document = cursor.current();
                    let id = document.get_str("uuid").unwrap();

                    log::error!("Object {} does not conform to the schema: {}", id, e);
                }
            }
        }

        Ok(results)
    }

    async fn object(&self, ctx: &Context<'_>, uuid: String, locale: Option<Locale>) -> GraphQLResult<Object> {
        if ODC_IGNORE.contains(&uuid) { return Err("Object not found".into()); }

        let database = ctx.data::<Database>().unwrap();
        let mut cursor = database.collection.find(doc! {
            "uuid": &uuid
        }, None).await.unwrap();

        while let Ok(res) = cursor.advance().await {
            if !res { break; }

            match cursor.deserialize_current() {
                Ok(mut object) => {
                    let document = cursor.current();
                    object.localize(document, locale);

                    return Ok(object);
                },
                Err(e) => {
                    let document = cursor.current();
                    let id = document.get_str("uuid").unwrap();

                    log::error!("Object {} does not conform to the schema: {}", id, e);
                }
            }
        }

        Err("Object not found".into())
    }
}

pub type ObjectSchema = Schema<Query, EmptyMutation, EmptySubscription>;