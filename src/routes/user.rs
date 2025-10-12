use axum::{
    Json, Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};

use crate::{loginsystem::{AuthSession, Credentials}, AppState};

async fn login(mut auth_session: AuthSession, Json(body): Json<Credentials>) -> impl IntoResponse {
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

async fn logout(mut auth_session: AuthSession) -> impl IntoResponse {
    match auth_session.logout().await {
        Ok(_) => (StatusCode::OK, "Logged out successfully").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to log out").into_response(),
    }
}

async fn profile() -> impl IntoResponse {
    //TODO: Return user profile information
    "User profile"
}

pub fn user_router() -> Router<AppState> {
    //TODO: Add suitable middleware for authentication
    Router::new()
        .route("/login", post(login))
        .route("/logout", get(logout))
        .route("/profile", get(profile))
}
