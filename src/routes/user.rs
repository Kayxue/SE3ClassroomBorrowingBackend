use axum::{
    Json, Router,
    response::IntoResponse,
    routing::{get, post},
};

use crate::loginsystem::Credentials;

async fn login(Json(body): Json<Credentials>) -> impl IntoResponse {
    "Logged in"
}

async fn profile() -> impl IntoResponse {
    "User profile"
}

pub fn user_router() -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/profile", get(profile))
}
