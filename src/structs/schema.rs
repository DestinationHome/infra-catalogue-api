use std::collections::BTreeMap;

use async_graphql::{Context, EmptySubscription, Error, Object as GraphQLObject, Result as GraphQLResult, Schema, futures_util::StreamExt};
use bson::doc;
use hmac::{digest::KeyInit, Hmac};
use jwt::{Header, AlgorithmType, Token, SignWithKey};
use sha2::Sha512;

use super::{api::Database, collection::{Collection, COLLECTION_SIZE}, object::Locale, user::User};
use crate::Object;

#[cfg(feature = "odc-ignore")]
use crate::ODC_IGNORE;

pub struct Query;
pub struct Mutation;

#[GraphQLObject]
impl Query {
    async fn search(
        &self, ctx: &Context<'_>,
        query: Option<String>, locale: Option<Locale>,
        limit: Option<u32>, skip: Option<u32>
    ) -> GraphQLResult<Vec<Object>> {
        let database = ctx.data::<Database>().unwrap();

        // let query_doc = match query {
        //     Some(ref query) => Some(doc! { "$text": { "$search": query } }),
        //     None => None,
        // };
        // let mut cursor = database.objects.find(query_doc, options).await.unwrap();

        let aggregation = vec![
            doc! { // Match the query
                "$match": doc! {
                    "$text": doc! {
                        "$search": query.unwrap_or_default()
                    }
                }
            },
            doc! { // Group by UUID
                "$group": doc! {
                    "_id": "$uuid",
                    "firstDocument": doc! { // Keep the first document
                        "$first": "$$ROOT"
                    }
                }
            },
            doc! { // Replace the root with the first document
                "$replaceRoot": doc! {
                    "newRoot": "$firstDocument"
                }
            },
            doc! { // Skip the first `skip` results
                "$skip": skip.unwrap_or(0)
            },
            doc! { // Limit the number of results
                "$limit": limit.unwrap_or(u32::MAX) as i64
            },
            doc! { // Add `for_locale` field for internal reference
                "$addFields": doc! {
                    "for_locale": locale.unwrap_or_default().to_string()
                }
            },
            doc! { // Sort the results by the `uuid` field
                "$sort": doc! {
                    "uuid": 1
                }
            }
        ];

        #[cfg(feature="odc-ignore")]
        // Add a filter for ignored objects to the match stage
        if let Some(ref query) = query {
            let doc = aggregation.first_mut().unwrap().as_document_mut().unwrap();
            doc.get_document_mut("$match").unwrap().insert("uuid", doc! { "$nin": ODC_IGNORE });
        }

        // Aggregate the results into unprocessed objects
        let results: Vec<Object> = database.objects
            .aggregate(aggregation, None).await.unwrap()
            .with_type::<Object>()
            .filter_map(|r| async { r.ok() })
            .collect::<Vec<Object>>().await;

        Ok(results)
    }

    async fn random(
        &self, ctx: &Context<'_>,
        locale: Option<Locale>, amount: Option<u32>
    ) -> GraphQLResult<Vec<Object>> {
        let database = ctx.data::<Database>().unwrap();

        // Amount bounds are 0 to 50
        let amount = amount
            .unwrap_or(50)
            .clamp(0, 50);

        let pipeline = vec![
            doc! { "$sample": { "size": amount } },
            doc! { "$addFields": { "for_locale": locale.unwrap_or_default().to_string() } }
        ];
        
        let results: Vec<Object> = database.objects
            .aggregate(pipeline, None).await.unwrap()
            .with_type::<Object>()
            .filter_map(|r| async { r.ok() })
            .collect::<Vec<Object>>().await;

        Ok(results)
    }

    async fn object(&self, ctx: &Context<'_>, uuid: String, locale: Option<Locale>) -> GraphQLResult<Object> {

        #[cfg(feature="odc-ignore")]
        if ODC_IGNORE.contains(&uuid) { return Err("Object not found".into()); }

        let database = ctx.data::<Database>().unwrap();
        let result = database.objects.find_one(doc! {
            "uuid": &uuid
        }, None).await.unwrap();

        result
            // Required for localisation
            .map(|mut r| { r.for_locale = locale.unwrap_or_default().to_string(); r })
            .ok_or("Object not found".into())
    }

    async fn collections(&self, ctx: &Context<'_>) -> GraphQLResult<Vec<Collection>> {
        let database = ctx.data::<Database>().unwrap();

        let results: Vec<Collection> = database.collections.find(doc! {}, None).await.unwrap()
            .with_type::<Collection>()
            .filter_map(|r| async { r.ok() })
            .collect::<Vec<Collection>>().await;
    
        Ok(results)
    }
}

#[GraphQLObject]
impl Mutation {
    #[cfg(debug_assertions)]
    async fn create_user(
        &self, ctx: &Context<'_>,
        display_name: String
    ) -> GraphQLResult<User> {
        let database = ctx.data::<Database>().unwrap();

        let private_key = std::env::var("JWT_PRIVATE_KEY")
            .map_err(|_| Error::new("Failed to get `JWT_PRIVATE_KEY` from environment!"))?;

        let uuid = bson::uuid::Uuid::new().to_string();

        // Create token
        let key: Hmac<Sha512> = Hmac::new_from_slice(private_key.as_bytes()).unwrap();

        let header = Header {
            algorithm: AlgorithmType::Hs512,
            ..Default::default()
        };

        let mut claims: BTreeMap<String, String> = BTreeMap::new();
        claims.insert("iss".to_string(), "API".to_string());
        claims.insert("sub".to_string(), uuid.clone());

        let token = Token::new(header, claims).sign_with_key(&key);

        if let Err(e) = token {
            log::error!("Failed to create an authentication token! {}", e);
            return Err(Error::new("Failed to create an authentication token!"));
        }

        let user = User {
            uuid,
            display_name,
            token: token.unwrap().as_str().to_string(),
        };

        database.users.insert_one(&user, None).await.unwrap();

        Ok(user)
    }

    async fn create_collection(
        &self, ctx: &Context<'_>,
        name: String, description: Option<String>, image_url: Option<String>, objects: Vec<String>
    ) -> GraphQLResult<Collection> {
        let database = ctx.data::<Database>().unwrap();
        let user = ctx.data::<User>()
            .map_err(|_| Error::new("Failed to get User from `Authorization` header!"))?;

        if !COLLECTION_SIZE.contains(&objects.len()) {
            return Err(Error::new(format!(
                "Collection must contain between {} and {} objects",
                COLLECTION_SIZE.start(),
                COLLECTION_SIZE.end()
            )));
        }

        let now = chrono::Utc::now();

        // Check every object to see if it exists
        let found_objects = Object::resolve_many(database, &objects).await
            .iter().map(|o| o.uuid.clone()).collect::<Vec<String>>();
        
        for uuid in objects {
            if !found_objects.contains(&uuid) {
                return Err(Error::new(format!("Object {} does not exist!", uuid)));
            }
        }

        let collection = Collection {
            uuid: bson::uuid::Uuid::new().to_string(),
            
            name,
            description,
            image_url,

            author: user.to_owned().uuid,
            objects: found_objects,

            created_at: now,
            updated_at: now,
        };

        database.collections.insert_one(&collection, None).await.unwrap();

        Ok(collection)
    }

    async fn delete_collection(
        &self, ctx: &Context<'_>,
        uuid: String
    ) -> GraphQLResult<Collection> {
        let database = ctx.data::<Database>().unwrap();
        let _ = ctx.data::<User>()
            .map_err(|_| Error::new("Failed to get User from `Authorization` header!"))?;

        // TODO: maybe make it so only the author can delete the collection?

        match database.collections.find_one_and_delete(doc! { "uuid": &uuid }, None).await.unwrap() {
            Some(collection) => Ok(collection),
            None => return Err(Error::new(format!("Collection {} does not exist!", uuid)))
        }
    }
}

pub type ObjectSchema = Schema<Query, Mutation, EmptySubscription>;