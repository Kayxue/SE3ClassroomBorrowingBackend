use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
};
use axum_login::login_required;
use redis::AsyncCommands;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    EntityTrait,
    prelude::DateTimeWithTimeZone,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    argon_hasher::{hash, verify},
    entities::{self, sea_orm_active_enums::Role, user},
    login_system::{AuthBackend, AuthSession, Credentials},
    utils::{check_student_id, get_redis_options},
};

use nanoid::nanoid;

#[derive(Serialize, Deserialize, ToSchema)]
pub struct RegisterBody {
    username: String,
    email: String,
    password: String,
    phone_number: String,
    name: String,
    student_id: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct UpdatePasswordBody {
    old_password: String,
    new_password: String,
    confirm: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub phone_number: String,
    pub role: Role,
    #[schema(value_type = String)]
    pub created_at: DateTimeWithTimeZone,
    #[schema(value_type = String)]
    pub updated_at: DateTimeWithTimeZone,
    pub name: String,
}

// ===============================
//   Update Profile Struct
// ===============================

#[derive(Serialize, Deserialize, ToSchema)]
pub struct UpdateProfileBody {
    pub username: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub name: Option<String>,
}

impl From<user::Model> for UserResponse {
    fn from(user: user::Model) -> Self {
        Self {
            id: user.id,
            username: user.username,
            email: user.email,
            phone_number: user.phone_number,
            role: user.role,
            created_at: user.created_at,
            updated_at: user.updated_at,
            name: user.name,
        }
    }
}

#[utoipa::path(
    post,
    tags = ["User"],
    description = "Register a new user",
    path = "/register",
    request_body(content = RegisterBody, description = "User registration data", content_type = "application/json"),
    responses(
        (status = 201, description = "User registered successfully", body = UserResponse),
        (status = 500, description = "Failed to create user", body = String),
    )
)]
pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterBody>,
) -> impl IntoResponse {
    let RegisterBody {
        username,
        email,
        password,
        phone_number,
        name,
        student_id,
    } = body;

    if !check_student_id(&student_id) {
        return (StatusCode::BAD_REQUEST, "Invalid student ID").into_response();
    }

    let hashed_password = hash(password).await.unwrap();

    let new_user = user::ActiveModel {
        id: Set(nanoid!()),
        username: Set(username),
        email: Set(email),
        password: Set(hashed_password),
        phone_number: Set(phone_number),
        role: Set(Role::User),
        created_at: NotSet,
        updated_at: NotSet,
        name: Set(name),
    };

    match new_user.insert(&state.db).await {
        Ok(user) => {
            // Cache the new user (ignore errors - caching is best effort)
            if let Ok(mut redis_client) = state.redis.get_multiplexed_async_connection().await {
                let _: Result<(), _> = redis_client
                    .set_options(
                        format!("user_{}", user.id),
                        serde_json::to_string(&user).unwrap(),
                        get_redis_options(),
                    )
                    .await;
            }

            let user_response = UserResponse::from(user);
            (StatusCode::CREATED, Json(user_response)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create user").into_response(),
    }
}

#[utoipa::path(
    post,
    tags = ["User"],
    description = "User login",
    path = "/login",
    request_body(content = Credentials, description = "User login credentials", content_type = "application/json"),
    responses(
        (status = 200, description = "User logged in successfully", body = UserResponse),
        (status = 401, description = "Invalid credentials", body = String),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn login(
    mut auth_session: AuthSession,
    Json(body): Json<Credentials>,
) -> impl IntoResponse {
    let user = match auth_session.authenticate(body).await {
        Ok(Some(user)) => user,
        Ok(None) => return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response(),
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    if auth_session.login(&user).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to log in").into_response();
    }

    let user_response = UserResponse::from(user);
    (StatusCode::OK, Json(user_response)).into_response()
}

#[utoipa::path(
    get,
    tags = ["User"],
    description = "User logout",
    path = "/logout",
    responses(
        (status = 200, description = "User logged out successfully"),
        (status = 500, description = "Failed to log out", body = String),
    )
)]
pub async fn logout(mut auth_session: AuthSession) -> impl IntoResponse {
    match auth_session.logout().await {
        Ok(_) => (StatusCode::OK, "Logged out successfully").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to log out").into_response(),
    }
}

#[utoipa::path(
    get,
    tags = ["User"],
    description = "Get user profile",
    path = "/profile",
    responses(
        (status = 200, description = "User profile retrieved successfully", body = UserResponse),
        (status = 401, description = "Unauthorized"),
    ),
    security(
        ("session_cookie" = [])
    )
)]
async fn profile(session: AuthSession) -> impl IntoResponse {
    let user_response = UserResponse::from(session.user.unwrap());
    (StatusCode::OK, Json(user_response)).into_response()
}

#[utoipa::path(
    get,
    tags = ["User"],
    description = "Get user by ID",
    path = "/{id}",
    params(
        ("id" = String, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User found", body = UserResponse),
        (status = 404, description = "User not found", body = String),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn get_user(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    // Try to get from cache first
    if let Ok(mut redis_client) = state.redis.get_multiplexed_async_connection().await {
        let user: Option<String> = redis_client.get(format!("user_{}", id)).await.unwrap_or(None);
        if let Some(user_str) = user {
            if let Ok(user) = serde_json::from_str::<entities::user::Model>(&user_str) {
                let user_response = UserResponse::from(user);
                return (StatusCode::OK, Json(user_response)).into_response();
            }
        }
    }

    // Fallback to database
    match user::Entity::find_by_id(id.clone()).one(&state.db).await {
        Ok(Some(user)) => {
            // Cache the result for future requests
            if let Ok(mut redis_client) = state.redis.get_multiplexed_async_connection().await {
                let _: Result<(), _> = redis_client
                    .set_options(
                        format!("user_{}", user.id),
                        serde_json::to_string(&user).unwrap(),
                        get_redis_options(),
                    )
                    .await;
            }
            let user_response = UserResponse::from(user);
            (StatusCode::OK, Json(user_response)).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "User not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch user").into_response(),
    }
}

#[utoipa::path(
    put,
    tags = ["User"],
    description = "Update user password",
    path = "/update-password",
    request_body(content = UpdatePasswordBody, description = "User password update data", content_type = "application/json"),
    responses(
        (status = 200, description = "Password updated successfully", body = String),
        (status = 400, description = "New password and confirm password are not same", body = String),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = String),
    ),
    security(
        ("session_cookie" = [])
    )
)]
pub async fn update_password(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<UpdatePasswordBody>,
) -> impl IntoResponse {
    let UpdatePasswordBody {
        old_password,
        new_password,
        confirm,
    } = body;
    if new_password != confirm {
        return (
            StatusCode::BAD_REQUEST,
            "New password and confirm password are not same",
        );
    }
    let user_current = session.user.unwrap();
    let old_hashed_password = &user_current.password;
    if verify(old_password, old_hashed_password).await.is_err() {
        return (StatusCode::BAD_REQUEST, "Old password is not correct");
    }

    let mut new_user: user::ActiveModel = user_current.into();
    let new_hashed_password = hash(new_password).await.unwrap();
    new_user.password = Set(new_hashed_password);
    match new_user.update(&state.db).await {
        Ok(updated_user) => {
            // Update cache (ignore errors - caching is best effort)
            if let Ok(mut redis_client) = state.redis.get_multiplexed_async_connection().await {
                let _: Result<(), _> = redis_client
                    .set_options(
                        format!("user_{}", updated_user.id),
                        serde_json::to_string(&updated_user).unwrap(),
                        get_redis_options(),
                    )
                    .await;
            }
            (StatusCode::OK, "Password updated successfully")
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update user password",
        ),
    }
}

// ===============================
//   Update Profile
// ===============================

#[utoipa::path(
    put,
    tags = ["User"],
    description = "Update user profile info",
    path = "/update-profile",
    request_body(
        content = UpdateProfileBody,
        description = "User profile update data",
        content_type = "application/json"
    ),
    responses(
        (status = 200, description = "Profile updated successfully", body = UserResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = String),
    ),
    security(("session_cookie" = []))
)]
pub async fn update_profile(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<UpdateProfileBody>,
) -> impl IntoResponse {
    let user_current = session.user.unwrap();

    let mut new_user: user::ActiveModel = user_current.into();

    if let Some(username) = body.username {
        new_user.username = Set(username);
    }
    if let Some(email) = body.email {
        new_user.email = Set(email);
    }
    if let Some(phone_number) = body.phone_number {
        new_user.phone_number = Set(phone_number);
    }
    if let Some(name) = body.name {
        new_user.name = Set(name);
    }

    match new_user.update(&state.db).await {
        Ok(updated_user) => {
            // Update cache (ignore errors - caching is best effort)
            if let Ok(mut redis_client) = state.redis.get_multiplexed_async_connection().await {
                let _: Result<(), _> = redis_client
                    .set_options(
                        format!("user_{}", updated_user.id),
                        serde_json::to_string(&updated_user).unwrap(),
                        get_redis_options(),
                    )
                    .await;
            }
            let user_response = UserResponse::from(updated_user);
            (StatusCode::OK, Json(user_response)).into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update profile",
        )
            .into_response(),
    }
}

pub fn user_router() -> Router<AppState> {
    let login_required_router = Router::new()
        .route("/profile", get(profile))
        .route("/update-password", put(update_password))
        .route("/update-profile", put(update_profile))
        .route_layer(login_required!(AuthBackend));

    Router::new()
        .route("/login", post(login))
        .route("/logout", get(logout))
        .route("/register", post(register))
        .route("/{id}", get(get_user))
        .merge(login_required_router)
}
