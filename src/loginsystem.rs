use sea_orm::DatabaseConnection;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

//TODO: Implement AuthUser trait for User model

pub struct Backend {
    db: DatabaseConnection,
}
//TODO: Implement AuthnBackend for Backend struct
