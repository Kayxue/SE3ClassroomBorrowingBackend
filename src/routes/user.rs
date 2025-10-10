use axum::{
    Json, Router,
    response::IntoResponse,
    routing::{get, post},
};

use crate::loginsystem::Credentials;

async fn login(Json(body): Json<Credentials>) -> impl IntoResponse {
    //TODO: Implement login logic
    "Logged in"
}

async fn logout() -> impl IntoResponse {
    //TODO: Implement logout logic
    "Logged out"
}

async fn profile() -> impl IntoResponse {
    //TODO: Return user profile information
    "User profile"
}

pub fn user_router() -> Router {
    //TODO: Add suitable middleware for authentication
    Router::new()
        .route("/login", post(login))
        .route("/logout", get(logout))
        .route("/profile", get(profile))
}
