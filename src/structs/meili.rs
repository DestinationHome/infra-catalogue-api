use base64::Engine as _;
use bson::doc;
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::api::Database;
use super::object::{
    ClothingType, FurnitureType, Gender, LocalizedEntry, Object, ObjectType, SceneType,
};
use super::schema::{
    ClothingTypeFacet, FurnitureTypeFacet, GenderFacet, ObjectConnection, ObjectEdge, ObjectFacets,
    ObjectSearchInput, ObjectSort, PageInfo, SceneTypeFacet, TypeFacet,
};

// ---------------------------------------------------------------------------
// Document shape ingested into Meilisearch
// ---------------------------------------------------------------------------

fn extract_localized_strings(entry: Option<&LocalizedEntry<String>>) -> (String, Vec<String>) {
    let mut default_str = String::new();
    let mut all_translations = Vec::new();

    if let Some(e) = entry {
        if let Some(ref d) = e.default {
            default_str = d.clone();
            if !d.trim().is_empty() {
                all_translations.push(d.clone());
            }
        }
        if let Some(ref loc) = e.localized {
            for v in loc.values() {
                if !v.trim().is_empty() && !all_translations.contains(v) {
                    all_translations.push(v.clone());
                }
            }
        }
    }

    (default_str, all_translations)
}

#[derive(Debug, Serialize)]
pub struct MeiliDocument {
    pub uuid: String,
    pub timestamp: u64,
    pub name: String,
    pub names: Vec<String>,
    pub description: String,
    pub descriptions: Vec<String>,
    pub maker: String,
    pub makers: Vec<String>,
    pub r#type: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clothing_type: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub furniture_type: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_type: Option<u64>,
    pub genders: Vec<u64>,
}

impl From<&Object> for MeiliDocument {
    fn from(obj: &Object) -> Self {
        let ts = obj
            .timestamp
            .as_ref()
            .and_then(|t| t.parse::<u64>().ok())
            .unwrap_or(0);

        let (name, names) = extract_localized_strings(obj.names.as_ref());
        let (description, descriptions) = extract_localized_strings(obj.descriptions.as_ref());
        let (maker, makers) = extract_localized_strings(obj.maker.as_ref());

        let (r#type, clothing_type, furniture_type, scene_type, genders) =
            obj.metadata.as_ref().map_or_else(
                || (5u64, None, None, None, vec![]),
                |m| {
                    (
                        m.r#type as u64,
                        m.clothing_type.map(|ct| ct as u64),
                        m.furniture_type.map(|ft| ft as u64),
                        m.scene_type.map(|st| st as u64),
                        m.genders
                            .as_ref()
                            .map(|g| g.iter().map(|v| *v as u64).collect())
                            .unwrap_or_default(),
                    )
                },
            );

        Self {
            uuid: obj.uuid.clone(),
            timestamp: ts,
            name,
            names,
            description,
            descriptions,
            maker,
            makers,
            r#type,
            clothing_type,
            furniture_type,
            scene_type,
            genders,
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP client (shared, connection-pooled)
// ---------------------------------------------------------------------------

lazy_static::lazy_static! {
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("Failed to build Meilisearch reqwest client");
}

// ---------------------------------------------------------------------------
// Index initialisation (called once on startup)
// ---------------------------------------------------------------------------

pub async fn init_meili_index(meili_url: &str, database: &Database) {
    let base = meili_url.trim_end_matches('/');

    // Create the index if it does not exist (idempotent)
    let _ = HTTP_CLIENT
        .post(format!("{base}/indexes"))
        .json(&serde_json::json!({ "uid": "odc", "primaryKey": "uuid" }))
        .send()
        .await;

    // Configure searchable, filterable, sortable, pagination, and typo attributes
    let settings = serde_json::json!({
        "searchableAttributes": [
            "name",
            "names",
            "description",
            "descriptions",
            "maker",
            "makers",
            "uuid"
        ],
        "filterableAttributes": ["type", "clothing_type", "furniture_type", "scene_type", "genders"],
        "sortableAttributes": ["name", "timestamp"],
        "pagination": {
            "maxTotalHits": 200000
        },
        "typoTolerance": {
            "enabled": true,
            "minWordSizeForTypos": {
                "oneTypo": 4,
                "twoTypos": 8
            }
        }
    });

    match HTTP_CLIENT
        .patch(format!("{base}/indexes/odc/settings"))
        .json(&settings)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            log::info!("Meilisearch odc index settings applied.");
        }
        Ok(resp) => {
            log::warn!(
                "Meilisearch settings update returned HTTP {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }
        Err(e) => {
            log::warn!("Meilisearch settings update failed: {e}");
        }
    }

    // Check if Meilisearch has documents; if empty, automatically sync from MongoDB
    sync_meili_from_mongodb_background(meili_url.to_string(), database.clone());
}

pub fn sync_meili_from_mongodb_background(meili_url: String, database: Database) {
    actix_web::rt::spawn(async move {
        let base = meili_url.trim_end_matches('/');
        let stats_url = format!("{base}/indexes/odc/stats");

        let mongo_count = database
            .objects
            .estimated_document_count(None)
            .await
            .unwrap_or(0);
        let meili_count = match HTTP_CLIENT.get(&stats_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let stats: serde_json::Value = resp.json().await.unwrap_or_default();
                stats
                    .get("numberOfDocuments")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0)
            }
            _ => 0,
        };

        if mongo_count > 0 && meili_count >= mongo_count {
            log::info!(
                "Meilisearch odc index is fully populated ({meili_count}/{mongo_count} docs)."
            );
            return;
        }

        log::info!(
            "Meilisearch needs synchronization ({meili_count}/{mongo_count} docs). Starting background sync from MongoDB..."
        );

        // Sort by timestamp ASC so newest versions of duplicate UUIDs overwrite older ones
        let find_options = FindOptions::builder()
            .sort(doc! { "timestamp": 1, "_id": 1 })
            .build();

        let mut cursor = match database.objects.find(None, find_options).await {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to query MongoDB objects for Meilisearch sync: {e}");
                return;
            }
        };

        use async_graphql::futures_util::StreamExt;
        let mut batch: Vec<MeiliDocument> = Vec::with_capacity(5000);
        let mut total = 0;

        while let Some(res) = cursor.next().await {
            match res {
                Ok(obj) => {
                    batch.push(MeiliDocument::from(&obj));
                    if batch.len() >= 5000 {
                        total += batch.len();
                        let _ = HTTP_CLIENT
                            .post(format!("{base}/indexes/odc/documents?primaryKey=uuid"))
                            .json(&batch)
                            .send()
                            .await;
                        batch.clear();
                        log::info!("Synced {total} objects to Meilisearch...");
                    }
                }
                Err(err) => {
                    log::warn!("Skipping malformed MongoDB object during sync: {err}");
                }
            }
        }

        if !batch.is_empty() {
            total += batch.len();
            let _ = HTTP_CLIENT
                .post(format!("{base}/indexes/odc/documents?primaryKey=uuid"))
                .json(&batch)
                .send()
                .await;
        }

        log::info!("Meilisearch initial sync complete ({total} objects indexed).");
    });
}

// ---------------------------------------------------------------------------
// Background write-through cache
// ---------------------------------------------------------------------------

pub fn index_objects_in_meili_background(meili_url: String, objects: &[Object]) {
    if objects.is_empty() || meili_url.trim().is_empty() {
        return;
    }
    let docs: Vec<MeiliDocument> = objects.iter().map(MeiliDocument::from).collect();
    actix_web::rt::spawn(async move {
        let endpoint = format!(
            "{}/indexes/odc/documents?primaryKey=uuid",
            meili_url.trim_end_matches('/')
        );
        match HTTP_CLIENT.post(&endpoint).json(&docs).send().await {
            Ok(resp) if resp.status().is_success() => {
                log::debug!("Cached {} objects into Meilisearch", docs.len());
            }
            Ok(resp) => {
                log::warn!(
                    "Meilisearch ingest returned HTTP {}: {}",
                    resp.status(),
                    resp.text().await.unwrap_or_default()
                );
            }
            Err(e) => {
                log::warn!("Meilisearch ingest failed: {e}");
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Cursor: base64-encoded page offset
// ---------------------------------------------------------------------------

pub fn encode_meili_cursor(offset: usize) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(offset.to_string())
}

pub fn decode_meili_cursor(cursor: &str) -> Option<usize> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .ok()?;
    String::from_utf8(bytes).ok()?.parse::<usize>().ok()
}

// ---------------------------------------------------------------------------
// Meilisearch search response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct MeiliHit {
    uuid: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeiliSearchResponse {
    #[serde(default)]
    hits: Vec<MeiliHit>,
    #[serde(default)]
    facet_distribution: HashMap<String, HashMap<String, u64>>,
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

fn build_meili_filter(input: &ObjectSearchInput) -> Vec<String> {
    let mut filters: Vec<String> = Vec::new();

    if let Some(types) = input.types.as_ref().filter(|v| !v.is_empty()) {
        let vals: Vec<String> = types
            .iter()
            .map(|t| format!("type = {}", *t as u8))
            .collect();
        filters.push(format!("({})", vals.join(" OR ")));
    }
    if let Some(types) = input.clothing_types.as_ref().filter(|v| !v.is_empty()) {
        let vals: Vec<String> = types
            .iter()
            .map(|t| format!("clothing_type = {}", *t as u8))
            .collect();
        filters.push(format!("({})", vals.join(" OR ")));
    }
    if let Some(types) = input.furniture_types.as_ref().filter(|v| !v.is_empty()) {
        let vals: Vec<String> = types
            .iter()
            .map(|t| format!("furniture_type = {}", *t as u8))
            .collect();
        filters.push(format!("({})", vals.join(" OR ")));
    }
    if let Some(types) = input.scene_types.as_ref().filter(|v| !v.is_empty()) {
        let vals: Vec<String> = types
            .iter()
            .map(|t| format!("scene_type = {}", *t as u8))
            .collect();
        filters.push(format!("({})", vals.join(" OR ")));
    }
    if let Some(genders) = input.genders.as_ref().filter(|v| !v.is_empty()) {
        let vals: Vec<String> = genders
            .iter()
            .map(|g| format!("genders = {}", *g as u8))
            .collect();
        filters.push(format!("({})", vals.join(" OR ")));
    }

    filters
}

fn build_meili_sort(sort: ObjectSort) -> Vec<String> {
    match sort {
        ObjectSort::Relevance => vec![],
        ObjectSort::NameAsc => vec!["name:asc".to_string()],
        ObjectSort::NameDesc => vec!["name:desc".to_string()],
        ObjectSort::Newest => vec!["timestamp:desc".to_string()],
        ObjectSort::Oldest => vec!["timestamp:asc".to_string()],
    }
}

fn parse_meili_facets(dist: &HashMap<String, HashMap<String, u64>>) -> ObjectFacets {
    let parse_u8 = |map: &HashMap<String, u64>| -> Vec<(u8, u64)> {
        map.iter()
            .filter_map(|(k, v)| k.parse::<u8>().ok().map(|n| (n, *v)))
            .collect()
    };

    let type_facets = dist.get("type").map_or_else(Vec::new, |m| {
        let mut items = parse_u8(m);
        items.sort_by_key(|(k, _)| *k);
        items
            .into_iter()
            .filter_map(|(k, v)| {
                ObjectType::try_from(k).ok().map(|t| TypeFacet {
                    r#type: t,
                    count: v,
                })
            })
            .collect()
    });

    let clothing_facets = dist.get("clothing_type").map_or_else(Vec::new, |m| {
        let mut items = parse_u8(m);
        items.sort_by_key(|(k, _)| *k);
        items
            .into_iter()
            .filter_map(|(k, v)| {
                ClothingType::try_from(k).ok().map(|t| ClothingTypeFacet {
                    clothing_type: t,
                    count: v,
                })
            })
            .collect()
    });

    let furniture_facets = dist.get("furniture_type").map_or_else(Vec::new, |m| {
        let mut items = parse_u8(m);
        items.sort_by_key(|(k, _)| *k);
        items
            .into_iter()
            .filter_map(|(k, v)| {
                FurnitureType::try_from(k).ok().map(|t| FurnitureTypeFacet {
                    furniture_type: t,
                    count: v,
                })
            })
            .collect()
    });

    let scene_facets = dist.get("scene_type").map_or_else(Vec::new, |m| {
        let mut items = parse_u8(m);
        items.sort_by_key(|(k, _)| *k);
        items
            .into_iter()
            .filter_map(|(k, v)| {
                SceneType::try_from(k).ok().map(|t| SceneTypeFacet {
                    scene_type: t,
                    count: v,
                })
            })
            .collect()
    });

    let gender_facets = dist.get("genders").map_or_else(Vec::new, |m| {
        let mut items = parse_u8(m);
        items.sort_by_key(|(k, _)| *k);
        items
            .into_iter()
            .filter_map(|(k, v)| {
                Gender::try_from(k).ok().map(|g| GenderFacet {
                    gender: g,
                    count: v,
                })
            })
            .collect()
    });

    ObjectFacets {
        types: type_facets,
        clothing_types: clothing_facets,
        furniture_types: furniture_facets,
        scene_types: scene_facets,
        genders: gender_facets,
    }
}

pub async fn search_meili(
    meili_url: &str,
    input: &ObjectSearchInput,
    limit: usize,
    after: Option<&str>,
    database: &Database,
) -> Result<ObjectConnection, String> {
    let offset = after.and_then(decode_meili_cursor).unwrap_or(0);
    let sort = input.sort.unwrap_or_default();
    let filter = build_meili_filter(input);
    let sort_rules = build_meili_sort(sort);

    let mut body = serde_json::json!({
        "q": input.query.as_deref().unwrap_or(""),
        "limit": limit + 1,
        "offset": offset,
        "facets": ["type", "clothing_type", "furniture_type", "scene_type", "genders"],
    });

    if !sort_rules.is_empty() {
        body["sort"] = serde_json::Value::Array(
            sort_rules
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );
    }

    if !filter.is_empty() {
        body["filter"] =
            serde_json::Value::Array(filter.into_iter().map(serde_json::Value::String).collect());
    }

    let endpoint = format!("{}/indexes/odc/search", meili_url.trim_end_matches('/'));
    let response = HTTP_CLIENT
        .post(&endpoint)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Meilisearch HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Meilisearch HTTP {status}: {body}"));
    }

    let resp: MeiliSearchResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Meilisearch response: {e}"))?;

    if resp.hits.is_empty() && offset == 0 {
        return Err("Meilisearch returned 0 hits, falling back to MongoDB".to_string());
    }

    let has_next_page = resp.hits.len() > limit;
    let page_hits = if has_next_page {
        &resp.hits[..limit]
    } else {
        &resp.hits[..]
    };

    let hit_uuids: Vec<String> = page_hits.iter().map(|h| h.uuid.clone()).collect();
    let hydrated = Object::resolve_many(database, &hit_uuids).await;
    let mut object_map: std::collections::HashMap<String, Object> =
        hydrated.into_iter().map(|o| (o.uuid.clone(), o)).collect();

    let for_locale = input.locale.unwrap_or_default();
    let mut edges = Vec::with_capacity(page_hits.len());

    for (i, hit) in page_hits.iter().enumerate() {
        if let Some(mut obj) = object_map.remove(&hit.uuid) {
            obj.for_locale = for_locale.to_string();
            let cursor = encode_meili_cursor(offset + i + 1);
            edges.push(ObjectEdge { cursor, node: obj });
        }
    }

    let start_cursor = edges.first().map(|e| e.cursor.clone());
    let end_cursor = edges.last().map(|e| e.cursor.clone());
    let facets = parse_meili_facets(&resp.facet_distribution);

    Ok(ObjectConnection {
        edges,
        page_info: PageInfo {
            has_next_page,
            has_previous_page: offset > 0,
            start_cursor,
            end_cursor,
        },
        facets: Some(facets),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_meili_cursor() {
        let cursor = encode_meili_cursor(42);
        let decoded = decode_meili_cursor(&cursor);
        assert_eq!(decoded, Some(42));
    }

    #[test]
    fn test_decode_invalid_cursor() {
        assert_eq!(decode_meili_cursor("!!!"), None);
    }

    #[test]
    fn test_build_meili_filter_empty() {
        let input = ObjectSearchInput::default();
        assert!(build_meili_filter(&input).is_empty());
    }
}
