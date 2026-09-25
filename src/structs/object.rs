#![allow(
    clippy::trait_duplication_in_bounds,
    reason = "GraphQL's SimpleObject proc-macro is doing heavy codegen and Clippy cannot tell"
)]

use std::{collections::BTreeMap, fmt::Display};

use async_graphql::{ComplexObject, Enum, OutputType, SimpleObject, futures_util::StreamExt};
use bson::doc;
use fancy_regex::Regex;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

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
    pub elements: Vec<Element>,
}

#[derive(Serialize, Deserialize, SimpleObject)]
#[graphql(rename_fields = "snake_case")]
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
#[graphql(rename_fields = "snake_case")]
pub struct AgeRating {
    pub minimum_age: u32,
    pub parental_control_level: u32,
}

#[derive(Serialize, Deserialize, SimpleObject, Debug)]
#[graphql(rename_fields = "snake_case")]
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

#[derive(Serialize, Deserialize, Enum, Clone, Copy, Eq, PartialEq, Default)]
pub enum Type {
    Reward,
    Premium,
    #[default]
    Other,
}

#[derive(Serialize_repr, Deserialize_repr, Enum, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u8)]
#[graphql(rename_items = "PascalCase")] // todo: fixme (inconsistent with others)
pub enum Gender {
    Male = 0,
    Female = 1,
}

impl TryFrom<u8> for Gender {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Male),
            1 => Ok(Self::Female),
            _ => Err(()),
        }
    }
}

#[derive(Serialize_repr, Deserialize_repr, Enum, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum SceneType {
    Apartment = 0,
    Clubhouse = 1,
}

impl TryFrom<u8> for SceneType {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Apartment),
            1 => Ok(Self::Clubhouse),
            _ => Err(()),
        }
    }
}

#[derive(Serialize, Deserialize, SimpleObject, Clone, Debug)]
#[graphql(rename_fields = "snake_case")]
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

#[derive(Serialize_repr, Deserialize_repr, Enum, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
pub enum ObjectType {
    Clothing = 0,
    Furniture = 1,
    Portable = 2,
    Scene = 3,
    Minigame = 4,
    Other = 5,
}

impl TryFrom<u8> for ObjectType {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Clothing),
            1 => Ok(Self::Furniture),
            2 => Ok(Self::Portable),
            3 => Ok(Self::Scene),
            4 => Ok(Self::Minigame),
            5 => Ok(Self::Other),
            _ => Err(()),
        }
    }
}

#[derive(Serialize_repr, Deserialize_repr, Enum, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
pub enum ClothingType {
    Hat = 0,
    Hair = 1,
    Jewelry = 2,
    Glasses = 3,
    Torso = 4,
    Hands = 5,
    Legs = 6,
    Feet = 7,
    Outfit = 8,
}

impl TryFrom<u8> for ClothingType {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Hat),
            1 => Ok(Self::Hair),
            2 => Ok(Self::Jewelry),
            3 => Ok(Self::Glasses),
            4 => Ok(Self::Torso),
            5 => Ok(Self::Hands),
            6 => Ok(Self::Legs),
            7 => Ok(Self::Feet),
            8 => Ok(Self::Outfit),
            _ => Err(()),
        }
    }
}

#[derive(Serialize_repr, Deserialize_repr, Enum, Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
pub enum FurnitureType {
    Appliance = 0,
    Chair = 1,
    Cube = 2,
    Flooring = 3,
    Footstool = 4,
    Frame = 5,
    Light = 6,
    Ornament = 7,
    Picture = 8,
    Sofa = 9,
    Storage = 10,
    Table = 11,
}

impl TryFrom<u8> for FurnitureType {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Appliance),
            1 => Ok(Self::Chair),
            2 => Ok(Self::Cube),
            3 => Ok(Self::Flooring),
            4 => Ok(Self::Footstool),
            5 => Ok(Self::Frame),
            6 => Ok(Self::Light),
            7 => Ok(Self::Ornament),
            8 => Ok(Self::Picture),
            9 => Ok(Self::Sofa),
            10 => Ok(Self::Storage),
            11 => Ok(Self::Table),
            _ => Err(()),
        }
    }
}

#[derive(Serialize, Deserialize, SimpleObject, Debug)]
pub struct LocalizedEntry<T: OutputType + Serialize> {
    pub default: Option<T>,
    pub localized: Option<BTreeMap<String, T>>, // en-US -> "text"
}

macro_rules! localized_field {
    ($self:ident, $field:ident, $locale:ident) => {
        match $locale {
            Locale::Default => $self.$field.as_ref().and_then(|f| f.default.clone()),
            _ => $self.$field.as_ref().and_then(|f| {
                f.localized
                    .as_ref()
                    .and_then(|l| l.get(&$locale.to_string()).cloned())
            }),
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
            l.age_rating.as_ref().and_then(|a| match locale {
                Locale::Default => a.default.clone(),
                _ => a
                    .localized
                    .as_ref()
                    .and_then(|l| l.get(&locale.to_string()).cloned()),
            })
        })
    }

    /// Check if the item might be a bundle, checking the name and description
    async fn bundle(&self) -> bool {
        let locale = Locale::from(self.for_locale.clone());

        let name = localized_field!(self, names, locale).unwrap_or_default();
        let description = localized_field!(self, descriptions, locale).unwrap_or_default();

        [name, description]
            .iter()
            .any(|s| BUNDLE_REGEX.is_match(s).unwrap())
    }

    /// Calculate the type of the object
    async fn r#type(&self) -> Type {
        if let Some(entitlements) = &self.entitlements
            && let Some(entitlement_id) = &entitlements.entitlement_id
        {
            let values = entitlement_id.iter().map(|e| e.value.clone());

            let is_reward = values.clone().any(|v| REWARD_REGEX.is_match(&v).unwrap());
            if is_reward {
                return Type::Reward;
            }

            let is_premium = values.clone().all(|v| PREMIUM_REGEX.is_match(&v).unwrap());
            if is_premium {
                return Type::Premium;
            }

            return Type::Other;
        }

        Type::Other
    }
}

impl Object {
    pub async fn resolve_many(database: &Database, uuids: &Vec<String>) -> Vec<Self> {
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
                    "for_locale": Locale::Default.to_string()
                }
            },
            doc! { // Sort the results by the `uuid` field
                "$sort": doc! {
                    "uuid": 1
                }
            },
        ];

        #[cfg(feature = "odc-ignore")]
        // Add a filter for ignored objects to the match stage
        if let Some(ref query) = query {
            let doc = aggregation.first_mut().unwrap().as_document_mut().unwrap();
            doc.get_document_mut("$match")
                .unwrap()
                .insert("uuid", doc! { "$nin": ODC_IGNORE });
        }

        // Aggregate the results into unprocessed objects
        let results: Vec<Self> = database
            .objects
            .aggregate(aggregation, None)
            .await
            .unwrap()
            .with_type::<Self>()
            .filter_map(|r| async { r.ok() })
            .collect::<Vec<Self>>()
            .await;

        results
    }
}

/// One of the films in the Star Wars Trilogy
#[derive(Enum, Copy, Clone, Eq, PartialEq, Default)]
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

    #[default]
    Default,
}

impl From<String> for Locale {
    fn from(s: String) -> Self {
        match s.as_str() {
            "en-GB" => Self::BritishEnglish,
            "en-US" => Self::AmericanEnglish,
            "en-SG" => Self::SingaporeEnglish,

            "it-IT" => Self::Italian,
            "de-DE" => Self::German,
            "es-ES" => Self::Spanish,
            "fr-FR" => Self::French,

            "ja-JP" => Self::Japanese,
            "ko-KR" => Self::Korean,
            "zh-HK" => Self::HongKongChinese,
            "zh-TW" => Self::TaiwaneseChinese,

            _ => Self::Default,
        }
    }
}

impl Display for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::BritishEnglish => "en-GB",
            Self::AmericanEnglish => "en-US",
            Self::SingaporeEnglish => "en-SG",

            Self::Italian => "it-IT",
            Self::German => "de-DE",
            Self::Spanish => "es-ES",
            Self::French => "fr-FR",

            Self::Japanese => "ja-JP",
            Self::Korean => "ko-KR",
            Self::HongKongChinese => "zh-HK",
            Self::TaiwaneseChinese => "zh-TW",

            Self::Default => "default",
        })
    }
}

impl Locale {
    pub fn path(&self) -> String {
        match self {
            Self::Default => "default".to_string(),
            _ => format!("localized.{}", self),
        }
    }

    pub fn iso_code(&self) -> String {
        match self {
            Self::Default => "default".to_string(),
            _ => self.to_string().split("-").last().unwrap().to_string(),
        }
    }
}
