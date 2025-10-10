use axum::{response::IntoResponse, routing::get, Router};

pub async fn profile() -> impl IntoResponse{
    "User profile"
}

pub fn user_router() -> Router{
    Router::new()
        .route("/profile", get(profile))
}