use std::collections::BTreeMap;

use async_graphql::{ComplexObject, Enum, OutputType, SimpleObject, futures_util::StreamExt};
use bson::doc;
use fancy_regex::Regex;
use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};
use lazy_static::lazy_static;

use super::api::Database;

// example: UP9000-NPUQ00020_00
const REGEX_PREMIUM_ITEM: &str = r"^[A-Z]{2}\d{4}-[A-Z]{4}\d{5}_\d{2}(?:-[A-Z0-9]{6})*$";
const REGEX_REWARD_ITEM: &str = r"^(?:LUA|AUTOMATIC)_REWARD$";
const REGEX_BUNDLE_ITEM: &str = r"(?i)(?:bundle|pack|set|collection)";

lazy_static! {
    static ref BUNDLE_REGEX: Regex = Regex::new(REGEX_BUNDLE_ITEM).unwrap();
    static ref PREMIUM_REGEX: Regex = Regex::new(REGEX_PREMIUM_ITEM).unwrap();
    static ref REWARD_REGEX: Regex = Regex::new(REGEX_REWARD_ITEM).unwrap();
}

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

#[derive(Serialize, Deserialize, SimpleObject, Clone, Debug)]
pub struct AgeRating {
    pub minimum_age: u32,
    pub parental_control_level: u32,
}

#[derive(Serialize, Deserialize, SimpleObject, Debug)]
pub struct Legal {
    pub age_rating: Option<LocalizedEntry<AgeRating>>,
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

#[derive(Serialize, Deserialize, SimpleObject, Debug)]
pub struct LocalizedEntry<T: OutputType + Serialize> {
    pub default: Option<T>,
    pub localized: Option<BTreeMap<String, T>> // en-US -> "text"
}

macro_rules! localized_field {
    ($self:ident, $field:ident, $locale:ident) => {
        match $locale {
            Locale::Default => $self.$field.as_ref().and_then(|f| f.default.clone()),
            _ => $self.$field.as_ref().and_then(|f| f.localized.as_ref().and_then(|l| l.get(&$locale.to_string()).cloned()))
        }
    };
}

#[derive(Serialize, Deserialize, SimpleObject)]
#[graphql(complex)] // Needed for `ComplexObject` derive
pub struct Object {
    pub uuid: String,
    pub version: Version,

    #[graphql(skip)]
    pub names: Option<LocalizedEntry<String>>,
    #[graphql(skip)]
    pub descriptions: Option<LocalizedEntry<String>>,
    #[graphql(skip)]
    pub maker: Option<LocalizedEntry<String>>,

    pub images: Option<Images>,

    pub data: Option<Data>,
    pub entitlements: Option<Entitlements>,

    #[graphql(skip)]
    pub legal: Option<Legal>,

    pub heat: Option<Heat>,
    pub timestamp: Option<String>,
    pub metadata: Option<Metadata>,

    #[graphql(skip)]
    #[serde(skip_deserializing)]
    pub for_locale: String,
}

#[ComplexObject]
impl Object {
    /// Compute a localized name for the object
    async fn name(&self) -> Option<String> {
        let locale = Locale::from(self.for_locale.clone());
        localized_field!(self, names, locale)
    }

    /// Compute a localized description for the object
    async fn description(&self) -> Option<String> {
        let locale = Locale::from(self.for_locale.clone());
        localized_field!(self, descriptions, locale)
    }

    /// Compute a localized maker for the object
    async fn maker(&self) -> Option<String> {
        let locale = Locale::from(self.for_locale.clone());
        localized_field!(self, maker, locale)
    }

    /// Compute a localized age rating for the object
    async fn legal(&self) -> Option<AgeRating> {
        let locale = Locale::from(self.for_locale.clone());

        self.legal.as_ref().and_then(|l| {
            l.age_rating.as_ref().and_then(|a| {
                match locale {
                    Locale::Default => a.default.clone(),
                    _ => a.localized.as_ref().and_then(|l| l.get(&locale.to_string()).cloned())
                }
            })
        })
    }

    /// Check if the item might be a bundle, checking the name and description
    async fn bundle(&self) -> bool {
        let locale = Locale::from(self.for_locale.clone());

        let name = localized_field!(self, names, locale).unwrap_or_default();
        let description = localized_field!(self, descriptions, locale).unwrap_or_default();

        let match_against = vec![name, description];
        let bundle = match_against.iter().any(|s| BUNDLE_REGEX.is_match(s).unwrap());

        bundle
    }

    /// Calculate the type of the object
    async fn r#type(&self) -> Type {
        if let Some(entitlements) = &self.entitlements {
            if let Some(entitlement_id) = &entitlements.entitlement_id {
                let values = entitlement_id.iter().map(|e| e.value.clone());

                let is_reward = values.clone().any(|v| REWARD_REGEX.is_match(&v).unwrap());
                if is_reward { return Type::Reward; }

                let is_premium = values.clone().all(|v| PREMIUM_REGEX.is_match(&v).unwrap());
                if is_premium { return Type::Premium; }

                return Type::Other;
            }
        }

        Type::Other
    }
}

impl Object {
    pub async fn resolve_many(database: &Database, uuids: &Vec<String>) -> Vec<Object> {
        let locale: Option<Locale> = None;
        let aggregation = vec![
            doc! { // Match the query
                "$match": doc! {
                    "uuid": doc! {
                        "$in": uuids
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

        results
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

impl From<String> for Locale {
    fn from(s: String) -> Self {
        match s.as_str() {
            "en-GB" => Locale::BritishEnglish,
            "en-US" => Locale::AmericanEnglish,
            "en-SG" => Locale::SingaporeEnglish,

            "it-IT" => Locale::Italian,
            "de-DE" => Locale::German,
            "es-ES" => Locale::Spanish,
            "fr-FR" => Locale::French,

            "ja-JP" => Locale::Japanese,
            "ko-KR" => Locale::Korean,
            "zh-HK" => Locale::HongKongChinese,
            "zh-TW" => Locale::TaiwaneseChinese,

            _ => Locale::Default
        }
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

impl Locale {
    pub fn path(&self) -> String {
        match self {
            Locale::Default => "default".to_string(),
            _ => format!("localized.{}", self.to_string())
        }
    }

    pub fn iso_code(&self) -> String {
        match self {
            Locale::Default => "default".to_string(),
            _ => self.to_string().split("-").last().unwrap().to_string()
        }
    }
}
