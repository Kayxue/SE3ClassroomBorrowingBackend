use crate::{
    argonhasher::verify,
    entities::{self, prelude::*, *},
};
use axum_login::{AuthUser, AuthnBackend, UserId};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Deserialize;
use utoipa::ToSchema;

pub type AuthSession = axum_login::AuthSession<AuthBackend>;

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

impl AuthUser for entities::user::Model {
    type Id = String;

    fn id(&self) -> Self::Id {
        self.id.to_owned()
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password.as_bytes()
    }
}

#[derive(Clone)]
pub struct AuthBackend {
    db: DatabaseConnection,
}

impl AuthBackend {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

impl AuthnBackend for AuthBackend {
    type User = entities::user::Model;
    type Credentials = Credentials;
    type Error = sea_orm::DbErr;

    async fn authenticate(
        &self,
        Credentials { email, password }: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user = User::find()
            .filter(user::Column::Email.eq(email))
            .one(&self.db)
            .await?;

        if let Some(ref user) = user {
            if verify(password.as_bytes(), &user.password).await.is_ok() {
                return Ok(Some(user.clone()));
            }
        }
        Ok(None)
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        let user = User::find_by_id(user_id.to_owned()).one(&self.db).await?;
        Ok(user)
    }
}
