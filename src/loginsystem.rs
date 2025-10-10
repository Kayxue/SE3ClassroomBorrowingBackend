use sea_orm::DatabaseConnection;
use serde::Deserialize;

pub type AuthSession = axum_login::AuthSession<AuthBackend>;

#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

//TODO: Implement AuthUser trait for User model

pub struct AuthBackend {
    db: DatabaseConnection,
}

impl AuthBackend {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

//TODO: Implement AuthnBackend for Backend struct
