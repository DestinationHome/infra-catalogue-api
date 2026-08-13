#[cfg(debug_assertions)]
use std::collections::BTreeMap;
use async_graphql::{
    futures_util::StreamExt, Context, EmptySubscription, Enum, Error, InputObject,
    Object as GraphQLObject, Result as GraphQLResult, Schema, SimpleObject,
};
use base64::Engine as _;
use bson::doc;
#[cfg(debug_assertions)]
use hmac::{digest::KeyInit, Hmac};
#[cfg(debug_assertions)]
use jwt::{AlgorithmType, Header, SignWithKey, Token};
#[cfg(debug_assertions)]
use sha2::Sha512;

use super::{
    api::Database,
    collection::{Collection, COLLECTION_SIZE},
    object::{ClothingType, FurnitureType, Gender, Locale, ObjectType, SceneType},
    meili::{index_objects_in_meili_background, search_meili},
    user::User,
};
use crate::Object;

#[cfg(feature = "odc-ignore")]
use crate::ODC_IGNORE;

pub struct Query;
pub struct Mutation;

const MAX_OBJECT_PAGE_SIZE: u32 = 100;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum ObjectSort {
    #[default]
    Relevance,
    NameAsc,
    NameDesc,
    Newest,
    Oldest,
}

/// Filters accepted by the catalogue connection.
#[derive(InputObject, Default, Clone)]
pub struct ObjectSearchInput {
    pub query: Option<String>,
    pub types: Option<Vec<ObjectType>>,
    pub clothing_types: Option<Vec<ClothingType>>,
    pub furniture_types: Option<Vec<FurnitureType>>,
    pub scene_types: Option<Vec<SceneType>>,
    pub genders: Option<Vec<Gender>>,
    pub locale: Option<Locale>,
    pub sort: Option<ObjectSort>,
}

#[derive(SimpleObject)]
pub struct PageInfo {
    pub has_next_page: bool,
    pub has_previous_page: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
}

#[derive(SimpleObject)]
pub struct ObjectEdge {
    pub cursor: String,
    pub node: Object,
}

#[derive(SimpleObject)]
pub struct TypeFacet {
    pub r#type: ObjectType,
    pub count: u64,
}

#[derive(SimpleObject)]
pub struct ClothingTypeFacet {
    pub clothing_type: ClothingType,
    pub count: u64,
}

#[derive(SimpleObject)]
pub struct FurnitureTypeFacet {
    pub furniture_type: FurnitureType,
    pub count: u64,
}

#[derive(SimpleObject)]
pub struct SceneTypeFacet {
    pub scene_type: SceneType,
    pub count: u64,
}

#[derive(SimpleObject)]
pub struct GenderFacet {
    pub gender: Gender,
    pub count: u64,
}

#[derive(SimpleObject)]
pub struct ObjectFacets {
    pub types: Vec<TypeFacet>,
    pub clothing_types: Vec<ClothingTypeFacet>,
    pub furniture_types: Vec<FurnitureTypeFacet>,
    pub scene_types: Vec<SceneTypeFacet>,
    pub genders: Vec<GenderFacet>,
}

#[derive(SimpleObject)]
pub struct ObjectConnection {
    pub edges: Vec<ObjectEdge>,
    pub page_info: PageInfo,
    pub facets: Option<ObjectFacets>,
}

fn escape_mongo_regex(query: &str) -> String {
    let mut escaped = String::with_capacity(query.len());
    for character in query.chars() {
        if matches!(
            character,
            '\\' | '^' | '$' | '.' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn encode_cursor(sort_val: &str, uuid: &str) -> String {
    let payload = format!("{}\x00{}", sort_val, uuid);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload)
}

fn decode_cursor(cursor: &str) -> Option<(String, String)> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .ok()?;
    let s = String::from_utf8(bytes).ok()?;
    let parts: Vec<&str> = s.split('\x00').collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        Some((String::new(), s))
    }
}

fn build_object_match(input: &ObjectSearchInput) -> bson::Document {
    let mut object_match = doc! {};

    if let Some(query) = input
        .query
        .as_deref()
        .map(str::trim)
        .filter(|query| !query.is_empty())
    {
        let escaped_query = escape_mongo_regex(query);
        object_match.insert(
            "$or",
            vec![
                doc! { "uuid": query },
                doc! { "names.default": { "$regex": escaped_query, "$options": "i" } },
            ],
        );
    }

    if let Some(types) = input.types.as_ref().filter(|values| !values.is_empty()) {
        object_match.insert(
            "metadata.type",
            doc! {
                "$in": types.iter().map(|value| *value as i32).collect::<Vec<i32>>()
            },
        );
    }
    if let Some(types) = input
        .clothing_types
        .as_ref()
        .filter(|values| !values.is_empty())
    {
        object_match.insert(
            "metadata.clothing_type",
            doc! {
                "$in": types.iter().map(|value| *value as i32).collect::<Vec<i32>>()
            },
        );
    }
    if let Some(types) = input
        .furniture_types
        .as_ref()
        .filter(|values| !values.is_empty())
    {
        object_match.insert(
            "metadata.furniture_type",
            doc! {
                "$in": types.iter().map(|value| *value as i32).collect::<Vec<i32>>()
            },
        );
    }
    if let Some(types) = input
        .scene_types
        .as_ref()
        .filter(|values| !values.is_empty())
    {
        object_match.insert(
            "metadata.scene_type",
            doc! {
                "$in": types.iter().map(|value| *value as i32).collect::<Vec<i32>>()
            },
        );
    }
    if let Some(genders) = input.genders.as_ref().filter(|values| !values.is_empty()) {
        object_match.insert(
            "metadata.genders",
            doc! {
                "$in": genders.iter().map(|value| *value as i32).collect::<Vec<i32>>()
            },
        );
    }

    #[cfg(feature = "odc-ignore")]
    {
        object_match.insert("uuid", doc! { "$nin": ODC_IGNORE.clone() });
    }

    object_match
}

fn get_sort_value(obj: &Object, sort: ObjectSort) -> String {
    match sort {
        ObjectSort::NameAsc | ObjectSort::NameDesc | ObjectSort::Relevance => obj
            .names
            .as_ref()
            .and_then(|n| n.default.clone())
            .unwrap_or_default(),
        ObjectSort::Newest | ObjectSort::Oldest => obj.timestamp.clone().unwrap_or_default(),
    }
}

fn build_sort_and_cursor(
    sort: ObjectSort,
    after: Option<&str>,
) -> (bson::Document, bson::Document) {
    let (sort_doc, sort_key_field, is_asc) = match sort {
        ObjectSort::NameAsc | ObjectSort::Relevance => (
            doc! { "names.default": 1, "uuid": 1 },
            "names.default",
            true,
        ),
        ObjectSort::NameDesc => (
            doc! { "names.default": -1, "uuid": -1 },
            "names.default",
            false,
        ),
        ObjectSort::Newest => (doc! { "timestamp": -1, "uuid": -1 }, "timestamp", false),
        ObjectSort::Oldest => (doc! { "timestamp": 1, "uuid": 1 }, "timestamp", true),
    };

    let cursor_match_doc = if let Some((sort_val, uuid)) = after.and_then(decode_cursor) {
        if sort_val.is_empty() {
            doc! { "uuid": { "$gt": uuid } }
        } else {
            let op = if is_asc { "$gt" } else { "$lt" };
            doc! {
                "$or": [
                    doc! { sort_key_field: doc! { op: &sort_val } },
                    doc! { sort_key_field: &sort_val, "uuid": doc! { op: &uuid } }
                ]
            }
        }
    } else {
        doc! {}
    };

    (sort_doc, cursor_match_doc)
}

fn parse_facet_items<T, F>(
    doc: &bson::Document,
    key: &str,
    parse_id: impl Fn(u8) -> Option<T>,
    make_facet: impl Fn(T, u64) -> F,
) -> Vec<F> {
    let mut result = Vec::new();
    if let Ok(array) = doc.get_array(key) {
        for item in array {
            if let Some(item_doc) = item.as_document() {
                let raw_id = item_doc
                    .get_i32("_id")
                    .ok()
                    .or_else(|| item_doc.get_i64("_id").ok().map(|v| v as i32));
                let count = item_doc
                    .get_i32("count")
                    .ok()
                    .map(|v| v as u64)
                    .or_else(|| item_doc.get_i64("count").ok().map(|v| v as u64));

                if let (Some(id), Some(cnt)) = (raw_id, count) {
                    if let Some(parsed) = parse_id(id as u8) {
                        result.push(make_facet(parsed, cnt));
                    }
                }
            }
        }
    }
    result
}

async fn find_object_page(
    database: &Database,
    input: &ObjectSearchInput,
    first: u32,
    after: Option<&str>,
) -> GraphQLResult<ObjectConnection> {
    if let Ok(meili_url) = std::env::var("MEILI_URL") {
        if !meili_url.trim().is_empty() {
            match search_meili(
                &meili_url,
                input,
                first as usize,
                after,
                database,
            )
            .await
            {
                Ok(connection) => return Ok(connection),
                Err(err) => {
                    log::warn!("Meilisearch search failed ({err}), falling back to MongoDB aggregation");
                }
            }
        }
    }

    let page_limit = (first + 1) as i64;
    let locale = input.locale.unwrap_or_default().to_string();
    let sort = input.sort.unwrap_or_default();

    let (sort_doc, cursor_match_doc) = build_sort_and_cursor(sort, after);
    let match_stage = build_object_match(input);

    let items_pipeline = vec![
        doc! { "$sort": sort_doc },
        doc! { "$match": cursor_match_doc },
        doc! { "$limit": page_limit },
        doc! { "$addFields": { "for_locale": locale } },
    ];

    let type_counts_pipeline = vec![
        doc! { "$match": { "metadata.type": { "$ne": null } } },
        doc! { "$group": { "_id": "$metadata.type", "count": { "$sum": 1 } } },
    ];

    let clothing_counts_pipeline = vec![
        doc! { "$match": { "metadata.clothing_type": { "$ne": null } } },
        doc! { "$group": { "_id": "$metadata.clothing_type", "count": { "$sum": 1 } } },
    ];

    let furniture_counts_pipeline = vec![
        doc! { "$match": { "metadata.furniture_type": { "$ne": null } } },
        doc! { "$group": { "_id": "$metadata.furniture_type", "count": { "$sum": 1 } } },
    ];

    let scene_counts_pipeline = vec![
        doc! { "$match": { "metadata.scene_type": { "$ne": null } } },
        doc! { "$group": { "_id": "$metadata.scene_type", "count": { "$sum": 1 } } },
    ];

    let gender_counts_pipeline = vec![
        doc! { "$unwind": "$metadata.genders" },
        doc! { "$group": { "_id": "$metadata.genders", "count": { "$sum": 1 } } },
    ];

    let aggregation = vec![
        doc! { "$match": match_stage },
        // Duplicate ODC documents exist in the current Mongo collection. Sort
        // first so grouping always retains the latest source record.
        doc! { "$sort": { "timestamp": -1, "_id": -1 } },
        doc! { "$group": { "_id": "$uuid", "doc": { "$first": "$$ROOT" } } },
        doc! { "$replaceRoot": { "newRoot": "$doc" } },
        doc! {
            "$facet": {
                "items": items_pipeline,
                "type_counts": type_counts_pipeline,
                "clothing_counts": clothing_counts_pipeline,
                "furniture_counts": furniture_counts_pipeline,
                "scene_counts": scene_counts_pipeline,
                "gender_counts": gender_counts_pipeline,
            }
        },
    ];

    let mut cursor = database
        .objects
        .aggregate(aggregation, None)
        .await
        .map_err(|error| Error::new(format!("Failed to search objects: {error}")))?;

    if let Some(result) = cursor.next().await {
        let facet_doc =
            result.map_err(|error| Error::new(format!("Database aggregation error: {error}")))?;

        let raw_items = facet_doc.get_array("items").cloned().unwrap_or_default();
        let mut objects: Vec<Object> = raw_items
            .into_iter()
            .filter_map(|bson_val| bson_val.as_document().cloned())
            .filter_map(|doc| bson::from_document::<Object>(doc).ok())
            .collect();

        let has_next_page = objects.len() > first as usize;
        if has_next_page {
            objects.pop();
        }

        let has_previous_page = after.is_some();

        if let Ok(meili_url) = std::env::var("MEILI_URL") {
            index_objects_in_meili_background(meili_url, &objects);
        }

        let edges: Vec<ObjectEdge> = objects
            .into_iter()
            .map(|obj| {
                let sort_val = get_sort_value(&obj, sort);
                let cursor_str = encode_cursor(&sort_val, &obj.uuid);
                ObjectEdge {
                    cursor: cursor_str,
                    node: obj,
                }
            })
            .collect();

        let start_cursor = edges.first().map(|e| e.cursor.clone());
        let end_cursor = edges.last().map(|e| e.cursor.clone());

        let type_facets = parse_facet_items::<ObjectType, TypeFacet>(
            &facet_doc,
            "type_counts",
            |val| ObjectType::try_from(val).ok(),
            |t, count| TypeFacet { r#type: t, count },
        );
        let clothing_facets = parse_facet_items::<ClothingType, ClothingTypeFacet>(
            &facet_doc,
            "clothing_counts",
            |val| ClothingType::try_from(val).ok(),
            |t, count| ClothingTypeFacet {
                clothing_type: t,
                count,
            },
        );
        let furniture_facets = parse_facet_items::<FurnitureType, FurnitureTypeFacet>(
            &facet_doc,
            "furniture_counts",
            |val| FurnitureType::try_from(val).ok(),
            |t, count| FurnitureTypeFacet {
                furniture_type: t,
                count,
            },
        );
        let scene_facets = parse_facet_items::<SceneType, SceneTypeFacet>(
            &facet_doc,
            "scene_counts",
            |val| SceneType::try_from(val).ok(),
            |t, count| SceneTypeFacet {
                scene_type: t,
                count,
            },
        );
        let gender_facets = parse_facet_items::<Gender, GenderFacet>(
            &facet_doc,
            "gender_counts",
            |val| Gender::try_from(val).ok(),
            |g, count| GenderFacet { gender: g, count },
        );

        let facets = ObjectFacets {
            types: type_facets,
            clothing_types: clothing_facets,
            furniture_types: furniture_facets,
            scene_types: scene_facets,
            genders: gender_facets,
        };

        Ok(ObjectConnection {
            edges,
            page_info: PageInfo {
                has_next_page,
                has_previous_page,
                start_cursor,
                end_cursor,
            },
            facets: Some(facets),
        })
    } else {
        Ok(ObjectConnection {
            edges: vec![],
            page_info: PageInfo {
                has_next_page: false,
                has_previous_page: after.is_some(),
                start_cursor: None,
                end_cursor: None,
            },
            facets: Some(ObjectFacets {
                types: vec![],
                clothing_types: vec![],
                furniture_types: vec![],
                scene_types: vec![],
                genders: vec![],
            }),
        })
    }
}

#[GraphQLObject]
impl Query {
    /// Browse or search unique catalogue objects using a stable forward cursor.
    async fn objects(
        &self,
        ctx: &Context<'_>,
        input: Option<ObjectSearchInput>,
        first: Option<u32>,
        after: Option<String>,
    ) -> GraphQLResult<ObjectConnection> {
        let database = ctx.data::<Database>().unwrap();
        let input = input.unwrap_or_default();
        let first = first.unwrap_or(24).clamp(1, MAX_OBJECT_PAGE_SIZE);

        find_object_page(database, &input, first, after.as_deref()).await
    }

    async fn search(
        &self,
        ctx: &Context<'_>,
        query: Option<String>,
        locale: Option<Locale>,
        limit: Option<u32>,
        skip: Option<u32>,
    ) -> GraphQLResult<Vec<Object>> {
        let database = ctx.data::<Database>().unwrap();

        let aggregation = vec![
            doc! {
                "$match": doc! {
                    "$text": doc! {
                        "$search": query.unwrap_or_default()
                    }
                }
            },
            doc! {
                "$group": doc! {
                    "_id": "$uuid",
                    "firstDocument": doc! {
                        "$first": "$$ROOT"
                    }
                }
            },
            doc! {
                "$replaceRoot": doc! {
                    "newRoot": "$firstDocument"
                }
            },
            doc! {
                "$skip": skip.unwrap_or(0)
            },
            doc! {
                "$limit": limit.unwrap_or(u32::MAX) as i64
            },
            doc! {
                "$addFields": doc! {
                    "for_locale": locale.unwrap_or_default().to_string()
                }
            },
            doc! {
                "$sort": doc! {
                    "uuid": 1
                }
            },
        ];

        #[cfg(feature = "odc-ignore")]
        if let Some(ref _query) = query {
            let doc = aggregation.first_mut().unwrap().as_document_mut().unwrap();
            doc.get_document_mut("$match")
                .unwrap()
                .insert("uuid", doc! { "$nin": ODC_IGNORE.clone() });
        }

        let results: Vec<Object> = database
            .objects
            .aggregate(aggregation, None)
            .await?
            .with_type::<Object>()
            .filter_map(|r| async { r.ok() })
            .collect::<Vec<Object>>()
            .await;

        Ok(results)
    }

    async fn random(
        &self,
        ctx: &Context<'_>,
        locale: Option<Locale>,
        amount: Option<u32>,
    ) -> GraphQLResult<Vec<Object>> {
        let database = ctx.data::<Database>().unwrap();

        let amount = amount.unwrap_or(50).clamp(0, 50);

        let pipeline = vec![
            doc! { "$sample": { "size": amount } },
            doc! { "$addFields": { "for_locale": locale.unwrap_or_default().to_string() } },
        ];

        let results: Vec<Object> = database
            .objects
            .aggregate(pipeline, None)
            .await
            .unwrap()
            .with_type::<Object>()
            .filter_map(|r| async { r.ok() })
            .collect::<Vec<Object>>()
            .await;

        Ok(results)
    }

    async fn object(
        &self,
        ctx: &Context<'_>,
        uuid: String,
        locale: Option<Locale>,
    ) -> GraphQLResult<Option<Object>> {
        #[cfg(feature = "odc-ignore")]
        if ODC_IGNORE.contains(&uuid) {
            return Ok(None);
        }

        let database = ctx.data::<Database>().unwrap();
        let options = mongodb::options::FindOneOptions::builder()
            .sort(doc! { "timestamp": -1, "_id": -1 })
            .build();

        let result = database
            .objects
            .find_one(doc! { "uuid": &uuid }, options)
            .await
            .map_err(|e| Error::new(format!("Database error: {e}")))?;

        Ok(result.map(|mut r| {
            r.for_locale = locale.unwrap_or_default().to_string();
            r
        }))
    }

    async fn collections(&self, ctx: &Context<'_>) -> GraphQLResult<Vec<Collection>> {
        let database = ctx.data::<Database>().unwrap();

        let results: Vec<Collection> = database
            .collections
            .find(doc! {}, None)
            .await
            .unwrap()
            .with_type::<Collection>()
            .filter_map(|r| async { r.ok() })
            .collect::<Vec<Collection>>()
            .await;

        Ok(results)
    }

    async fn collection(&self, ctx: &Context<'_>, uuid: String) -> GraphQLResult<Collection> {
        let database = ctx.data::<Database>().unwrap();
        let result = database
            .collections
            .find_one(doc! { "uuid": &uuid }, None)
            .await
            .unwrap();

        result.ok_or_else(|| "Collection not found".into())
    }
}

#[GraphQLObject]
impl Mutation {
    #[cfg(debug_assertions)]
    async fn create_user(&self, ctx: &Context<'_>, display_name: String) -> GraphQLResult<User> {
        let database = ctx.data::<Database>().unwrap();

        let private_key = std::env::var("JWT_PRIVATE_KEY")
            .map_err(|_| Error::new("Failed to get `JWT_PRIVATE_KEY` from environment!"))?;

        let uuid = bson::uuid::Uuid::new().to_string();

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
        &self,
        ctx: &Context<'_>,
        name: String,
        description: Option<String>,
        image_url: Option<String>,
        objects: Vec<String>,
    ) -> GraphQLResult<Collection> {
        let database = ctx.data::<Database>().unwrap();
        let user = ctx
            .data::<User>()
            .map_err(|_| Error::new("Failed to get User from `Authorization` header!"))?;

        if !COLLECTION_SIZE.contains(&objects.len()) {
            return Err(Error::new(format!(
                "Collection must contain between {} and {} objects",
                COLLECTION_SIZE.start(),
                COLLECTION_SIZE.end()
            )));
        }

        let now = chrono::Utc::now();

        let found_objects = Object::resolve_many(database, &objects)
            .await
            .iter()
            .map(|o| o.uuid.clone())
            .collect::<Vec<String>>();

        for uuid in &objects {
            if !found_objects.contains(uuid) {
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

        database
            .collections
            .insert_one(&collection, None)
            .await
            .unwrap();

        Ok(collection)
    }

    async fn delete_collection(
        &self,
        ctx: &Context<'_>,
        uuid: String,
    ) -> GraphQLResult<Collection> {
        let database = ctx.data::<Database>().unwrap();
        let _ = ctx
            .data::<User>()
            .map_err(|_| Error::new("Failed to get User from `Authorization` header!"))?;

        database
            .collections
            .find_one_and_delete(doc! { "uuid": &uuid }, None)
            .await
            .unwrap()
            .map_or_else(
                || Err(Error::new(format!("Collection {} does not exist!", uuid))),
                Ok,
            )
    }
}

pub type ObjectSchema = Schema<Query, Mutation, EmptySubscription>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_mongo_regex() {
        assert_eq!(escape_mongo_regex("hello world"), "hello world");
        assert_eq!(
            escape_mongo_regex("item (v1.0) [bundle]"),
            "item \\(v1\\.0\\) \\[bundle\\]"
        );
    }

    #[test]
    fn test_encode_decode_cursor() {
        let cursor = encode_cursor("Shirt", "uuid-1234");
        let decoded = decode_cursor(&cursor);
        assert_eq!(
            decoded,
            Some(("Shirt".to_string(), "uuid-1234".to_string()))
        );
    }

    #[test]
    fn test_build_object_match_empty() {
        let input = ObjectSearchInput::default();
        let doc = build_object_match(&input);
        assert!(doc.is_empty());
    }

    #[test]
    fn test_build_object_match_query_and_types() {
        let input = ObjectSearchInput {
            query: Some("Hat".to_string()),
            types: Some(vec![ObjectType::Clothing]),
            clothing_types: Some(vec![ClothingType::Hat]),
            ..Default::default()
        };
        let doc = build_object_match(&input);
        assert!(doc.contains_key("$or"));
        assert!(doc.contains_key("metadata.type"));
        assert!(doc.contains_key("metadata.clothing_type"));
    }
}
