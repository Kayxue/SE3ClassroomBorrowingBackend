use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

//TODO: Implement AuthUser trait for User model

//TODO: Declare Backend struct
//TODO: Implement AuthnBackend for Backend struct
