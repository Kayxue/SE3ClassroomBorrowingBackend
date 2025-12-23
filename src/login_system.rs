use crate::{
    argon_hasher::verify,
    entities::{self, prelude::*, sea_orm_active_enums::Role, *},
    utils::{REDIS_EXPIRY, get_redis_options},
};
use axum_login::{AuthUser, AuthnBackend, AuthzBackend, UserId};
use redis::{AsyncCommands, aio::MultiplexedConnection};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Deserialize;
use tracing::warn;
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
    redis: MultiplexedConnection,
}

impl AuthBackend {
    pub fn new(db: DatabaseConnection, redis: MultiplexedConnection) -> Self {
        Self { db, redis }
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
                // Cache user on successful login (ignore errors - caching is best effort)
                let mut redis = self.redis.clone();
                let result: Result<(), redis::RedisError> = redis
                    .set_options(
                        format!("user_{}", user.id),
                        serde_json::to_string(user).unwrap(),
                        get_redis_options(),
                    )
                    .await;
                if let Err(e) = result {
                    warn!("Failed to cache user {} in Redis during login: {}", user.id, e);
                }
                return Ok(Some(user.clone()));
            }
        }
        Ok(None)
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        // Clone connection once for this handler
        let mut redis = self.redis.clone();
        
        // Try to get from cache first
        let cached_user: Option<String> = match redis
            .get_ex(format!("user_{}", user_id.to_owned()), REDIS_EXPIRY)
            .await
        {
            Ok(user) => user,
            Err(e) => {
                warn!("Failed to get user {} from Redis cache: {}", user_id, e);
                None
            }
        };
        
        if let Some(user_str) = cached_user {
            if let Ok(user) = serde_json::from_str::<entities::user::Model>(&user_str) {
                return Ok(Some(user));
            }
        }

        // Fallback to database
        let user = User::find_by_id(user_id.to_owned()).one(&self.db).await?;

        // Cache the result for future requests (ignore errors - caching is best effort)
        if let Some(user) = &user {
            let result: Result<(), redis::RedisError> = redis
                .set_options(
                    format!("user_{}", user_id.to_owned()),
                    serde_json::to_string(user).unwrap(),
                    get_redis_options(),
                )
                .await;
            if let Err(e) = result {
                warn!("Failed to cache user {} in Redis: {}", user_id, e);
            }
        }
        Ok(user)
    }
}

impl AuthzBackend for AuthBackend {
    type Permission = Role;

    async fn has_perm(
        &self,
        user: &Self::User,
        perm: Self::Permission,
    ) -> Result<bool, Self::Error> {
        Ok(user.role == perm)
    }
}
