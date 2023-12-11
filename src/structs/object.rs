use async_graphql::{SimpleObject, Enum};
use bson::RawDocument;
use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};

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

#[derive(Serialize, Deserialize, SimpleObject)]
pub struct Metadata {
    pub r#type: ObjectType,

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
    #[serde(skip_deserializing)]
    pub r#type: Type,

    #[serde(skip_deserializing)]
    pub name: Option<String>,
    #[serde(skip_deserializing)]
    pub description: Option<String>,
    #[serde(skip_deserializing)]
    pub maker: Option<String>,

    pub images: Option<Images>,

    pub data: Option<Data>,
    pub entitlements: Option<Entitlements>,

    #[serde(skip_deserializing)]
    pub legal: Option<Legal>,

    pub heat: Option<Heat>,
    pub timestamp: Option<String>,
    pub metadata: Option<Metadata>
}

impl Object {
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

        if let Some(entitlements) = &mut self.entitlements {
            if let Some(entitlement_id) = &mut entitlements.entitlement_id {
                if entitlement_id.iter_mut().any(|e| vec!["LUA_REWARD", "AUTOMATIC_REWARD"].contains(&e.value.as_str())) {
                    self.r#type = Type::Reward;
                } else if entitlement_id.iter_mut().any(|e| e.value.len() == 26) {
                    self.r#type = Type::Premium;
                }
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
