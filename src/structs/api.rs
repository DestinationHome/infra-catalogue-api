use bson::doc;
use mongodb::{IndexModel, options::IndexOptions};

use super::{collection::Collection, object::Object, user::User};

#[derive(Clone)]
pub struct Database {
    pub objects: mongodb::Collection<Object>,
    pub collections: mongodb::Collection<Collection>,
    pub users: mongodb::Collection<User>,
}

impl Database {
    pub async fn ensure_indexes(&self) -> Result<(), mongodb::error::Error> {
        let unique_options = IndexOptions::builder().unique(true).build();

        // 1. odc (objects) indexes
        let object_indexes = vec![
            // NOTE: UUID might not be unique (i.e. different versions of the same object)
            IndexModel::builder().keys(doc! { "uuid": 1 }).build(),
            IndexModel::builder()
                .keys(doc! { "timestamp": -1, "uuid": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "type": 1, "timestamp": -1, "uuid": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "clothing_type": 1, "timestamp": -1, "uuid": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "furniture_type": 1, "timestamp": -1, "uuid": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "scene_type": 1, "timestamp": -1, "uuid": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "genders": 1, "timestamp": -1, "uuid": 1 })
                .build(),
            IndexModel::builder()
                .keys(doc! { "name.default": 1, "uuid": 1 })
                .build(),
        ];
        self.objects.create_indexes(object_indexes, None).await?;

        // 2. odc_collections indexes
        let collection_indexes = vec![
            IndexModel::builder()
                .keys(doc! { "uuid": 1 })
                .options(unique_options.clone())
                .build(),
            IndexModel::builder()
                .keys(doc! { "user_uuid": 1, "timestamp": -1 })
                .build(),
        ];
        self.collections
            .create_indexes(collection_indexes, None)
            .await?;

        // 3. odc_users indexes
        let user_indexes = vec![
            IndexModel::builder()
                .keys(doc! { "uuid": 1 })
                .options(unique_options.clone())
                .build(),
        ];
        self.users.create_indexes(user_indexes, None).await?;

        log::info!("MongoDB indexes verified successfully.");
        Ok(())
    }
}
