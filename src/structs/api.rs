use super::{object::Object, collection::Collection, user::User};

#[derive(Clone)]
pub struct Database {
    pub objects: mongodb::Collection<Object>,
    pub collections: mongodb::Collection<Collection>,
    pub users: mongodb::Collection<User>
}