use async_graphql::{SimpleObject, Enum};
use bson::{doc, RawDocument};
use fancy_regex::Regex;
use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};

use super::api::Database;

// example: UP9000-NPUQ00020_00
const REGEX_PREMIUM_ITEM: &str = r"^[A-Z]{2}\d{4}-[A-Z]{4}\d{5}_\d{2}(?:-[A-Z0-9]{6})*$";
const REGEX_REWARD_ITEM: &str = r"^(?:LUA|AUTOMATIC)_REWARD$";
const REGEX_BUNDLE_ITEM: &str = r"(?i)(?:bundle|pack|set|collection)";

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct Version {
    pub odc: String,
    pub hdk: Option<String>,
    pub object: Option<String>,
}

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct Images {
    pub large: Option<String>,
    pub small: Option<String>,
    pub maker: Option<String>,
}

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct Element {
    pub name: String,
    pub element: String,
}

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct Data {
    pub component: String,
    pub elements: Vec<Element>
}

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct Entitlements {
    pub entitlement_id: Option<Vec<EntitlementEntry>>,
    pub category_id: Option<Vec<EntitlementEntry>>,
    pub product_id: Option<Vec<EntitlementEntry>>,
}

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct EntitlementEntry {
    pub territory: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct AgeRating {
    pub minimum_age: u32,
    pub parental_control_level: u32,
}

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct Legal {
    pub age_rating: Option<AgeRating>,
}

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct Heat {
    pub main: Option<u32>,
    pub host: Option<u32>,
    pub vram: Option<u32>,
    pub ppu: Option<u32>,
    pub net: Option<u32>,
}

#[derive(Serialize, Deserialize, Enum, Clone, Copy, Eq, PartialEq)]
pub enum Type {
    Reward,
    Premium,
    Other,
}

impl Default for Type {
    fn default() -> Self {
        Type::Other
    }
}

#[derive(Serialize_repr, Deserialize_repr, Enum, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum Gender {
    Male,
    Female
}

#[derive(Serialize_repr, Deserialize_repr, Enum, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum SceneType {
    Apartment,
    Clubhouse
}

#[derive(Serialize, Deserialize, SimpleObject, Clone)]
pub struct Metadata {
    pub r#type: ObjectType,
    pub bundle: Option<bool>,

    // Furniture
    pub furniture_type: Option<FurnitureType>,

    // Clothing
    pub clothing_type: Option<ClothingType>,
    pub genders: Option<Vec<Gender>>,

    // Scenes
    pub scene_type: Option<SceneType>,
}

#[derive(Serialize_repr, Deserialize_repr, Enum, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum ObjectType {
    CLOTHING = 0,
    FURNITURE = 1,
    PORTABLE = 2,
    SCENE = 3,
    MINIGAME = 4,
    OTHER = 5
}

#[derive(Serialize_repr, Deserialize_repr, Enum, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum ClothingType {
    HAT = 0,
    HAIR = 1,
    JEWELRY = 2,
    GLASSES = 3,
    TORSO = 4,
    HANDS = 5,
    LEGS = 6,
    FEET = 7,
    OUTFIT = 8
}

#[derive(Serialize_repr, Deserialize_repr, Enum, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum FurnitureType {
    APPLIANCE = 0,
    CHAIR = 1,
    CUBE = 2,
    FLOORING = 3,
    FOOTSTOOL = 4,
    FRAME = 5,
    LIGHT = 6,
    ORNAMENT = 7,
    PICTURE = 8,
    SOFA = 9,
    STORAGE = 10,
    TABLE = 11
}

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct Object {
    pub uuid: String,

    pub version: Version,
    #[serde(skip_deserializing)] // Calculated at query-time
    pub r#type: Type,

    #[serde(skip_deserializing)] // Calculated at query-time
    pub bundle: bool,

    #[serde(skip_deserializing)] // Depends on requested locale
    pub name: Option<String>,
    #[serde(skip_deserializing)] // Depends on requested locale
    pub description: Option<String>,
    #[serde(skip_deserializing)] // Depends on requested locale
    pub maker: Option<String>,

    pub images: Option<Images>,

    pub data: Option<Data>,
    pub entitlements: Option<Entitlements>,

    #[serde(skip_deserializing)] // Re-formatted at query-time
    pub legal: Option<Legal>,

    pub heat: Option<Heat>,
    pub timestamp: Option<String>,
    pub metadata: Option<Metadata>
}

impl Object {
    pub async fn resolve_many(database: &Database, uuids: &Vec<String>) -> Vec<Object> {
        let mut cursor = database.objects.find(doc! { "uuid": { "$in": uuids } }, None).await.unwrap();
        let mut objects = Vec::new();

        while let Ok(res) = cursor.advance().await {
            if!res { break; } // No more requests

            match cursor.deserialize_current() {
                Ok(mut o) => {
                    o.complete(cursor.current(), None);
                    objects.push(o);
                },
                Err(e) => {
                    let document = cursor.current();
                    let id = document.get_str("uuid").unwrap();

                    log::error!("Object {} does not conform to the schema: {}", id, e);
                }
            }
        }

        // Retain only unique UUIDs
        objects.sort_by_key(|u| u.uuid.clone());
        objects.dedup_by_key(|u| u.uuid.clone());

        objects
    }

    fn extract_str(document: &RawDocument, key: &str, locale: &str) -> Option<String> {
        let mut document = document.get_document(key).ok();

        if locale != "default" {
            document = document.and_then(|doc| doc.get_document("localized").ok());
        }

        document.and_then(|doc| doc.get_str(locale).ok()).map(|s| s.to_string())
    }

    fn extract_obj<'a, T: for<'de> Deserialize<'de>>(document: &'a RawDocument, key: &str, locale: &str) -> Option<T> {
        let parts = key.split('.');

        let mut document: Option<&RawDocument> = Some(document);
        for part in parts {
            document = document.and_then(|doc| doc.get_document(part).ok());
        }

        if locale != "default" {
            document = document.and_then(|doc| doc.get_document("localized").ok());
        }

        document = document.and_then(|doc| doc.get_document(locale).ok());

        let parsed = document.and_then(|doc| bson::to_bson(&doc).ok()).unwrap_or_default();
        bson::from_bson::<T>(parsed).ok()
    }

    pub fn complete(&mut self, raw: &RawDocument, locale: Option<Locale>) {
        let iso_code = locale.unwrap_or_default().to_string();

        self.name = Object::extract_str(raw, "names", &iso_code)
            .or(Object::extract_str(raw, "names", "default"));

        self.description = Object::extract_str(raw, "descriptions", &iso_code)
            .or(Object::extract_str(raw, "descriptions", "default"));

        self.maker = Object::extract_str(raw, "maker", &iso_code)
            .or(Object::extract_str(raw, "maker", "default"));

        if raw.get_document("legal").is_ok() {
            self.legal = Some(Legal {
                age_rating: Object::extract_obj(raw, "legal.age_rating", &iso_code)
                    .or(Object::extract_obj(raw, "legal.age_rating", "default")),
            });
        }

        // Check if the item might be a bundle, checking the name and description
        let match_against = vec![
            self.name.to_owned().unwrap_or_default(),
            self.description.to_owned().unwrap_or_default()
        ];
        let bundle_regex = Regex::new(REGEX_BUNDLE_ITEM).unwrap();
        self.bundle = match_against.iter().any(|s| bundle_regex.is_match(s).unwrap());

        if let Some(entitlements) = &mut self.entitlements {
            if let Some(entitlement_id) = &mut entitlements.entitlement_id {
                let values = entitlement_id.iter().map(|e| e.value.clone());

                let premium_regex = Regex::new(REGEX_PREMIUM_ITEM).unwrap();
                let reward_regex = Regex::new(REGEX_REWARD_ITEM).unwrap();

                let mut r#type = Type::Other;

                let is_reward = values.clone().any(|v| reward_regex.is_match(&v).unwrap());
                let is_premium = values.clone().all(|v| premium_regex.is_match(&v).unwrap());
                
                if is_reward {
                    r#type = Type::Reward;
                } else if is_premium {
                    r#type = Type::Premium;
                }

                self.r#type = r#type;
            }
        }
    }
}


/// One of the films in the Star Wars Trilogy
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum Locale {
    BritishEnglish,
    AmericanEnglish,
    SingaporeEnglish,

    Italian,
    German,
    Spanish,
    French,

    Japanese,
    Korean,
    HongKongChinese,
    TaiwaneseChinese,

    Default,
}

impl Default for Locale {
    fn default() -> Self {
        Locale::Default
    }
}

impl ToString for Locale {
    fn to_string(&self) -> String {
        match self {
            Locale::BritishEnglish => "en-GB",
            Locale::AmericanEnglish => "en-US",
            Locale::SingaporeEnglish => "en-SG",

            Locale::Italian => "it-IT",
            Locale::German => "de-DE",
            Locale::Spanish => "es-ES",
            Locale::French => "fr-FR",

            Locale::Japanese => "ja-JP",
            Locale::Korean => "ko-KR",
            Locale::HongKongChinese => "zh-HK",
            Locale::TaiwaneseChinese => "zh-TW",

            Locale::Default => "default",
        }.to_string()
    }
}
