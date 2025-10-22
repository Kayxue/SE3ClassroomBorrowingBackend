use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use axum_login::login_required;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    argonhasher::hash,
    entities::{sea_orm_active_enums::Role, user},
    loginsystem::{AuthBackend, AuthSession, Credentials},
};

use nanoid::nanoid;

#[derive(Serialize, Deserialize, ToSchema)]
pub struct RegisterBody {
    username: String,
    email: String,
    password: String,
    phone_number: String,
}

#[utoipa::path(
    post,
    description = "Register a new user",
    path = "/user/register",
    request_body(content = RegisterBody, description = "User registration data", content_type = "application/json"),
    responses(
        (status = 201, description = "User registered successfully", body = user::Model),
        (status = 500, description = "Failed to create user"),
    )
)]
pub async fn register(
    State(state): State<AppState>,
    Json(RegisterBody {
        username,
        email,
        password,
        phone_number,
    }): Json<RegisterBody>,
) -> impl IntoResponse {
    let hashed_password = hash(password.as_bytes()).await.unwrap();

    let new_user = user::ActiveModel {
        id: Set(nanoid!()),
        username: Set(username),
        email: Set(email),
        password: Set(hashed_password),
        phone_number: Set(phone_number),
        role: Set(Role::User),
        created_at: NotSet,
        updated_at: NotSet,
    };

    match new_user.insert(&state.db).await {
        Ok(user) => (StatusCode::CREATED, Json(user)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create user").into_response(),
    }
}

#[utoipa::path(
    post,
    description = "User login",
    path = "/user/login",
    request_body(content = Credentials, description = "User login credentials", content_type = "application/json"),
    responses(
        (status = 200, description = "User logged in successfully", body = user::Model),
        (status = 401, description = "Invalid credentials"),
        (status = 500, description = "Internal server error"),
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

    (StatusCode::OK, Json(user)).into_response()
}

#[utoipa::path(
    get,
    description = "User logout",
    path = "/user/logout",
    responses(
        (status = 200, description = "User logged out successfully"),
        (status = 500, description = "Failed to log out"),
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
    description = "Get user profile",
    path = "/user/profile",
    responses(
        (status = 200, description = "User profile retrieved successfully", body = user::Model),
        (status = 401, description = "Unauthorized"),
    ),
    security(
        ("session_cookie" = [])
    )
)]
async fn profile(session: AuthSession) -> impl IntoResponse {
    (StatusCode::OK, Json(session.user.unwrap())).into_response()
}

pub fn user_router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", get(logout))
        .route(
            "/profile",
            get(profile).route_layer(login_required!(AuthBackend)),
        )
        .route("/register", post(register))
}
