use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete},
};
use axum_login::permission_required;
use sea_orm::{EntityTrait};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    entities::key,
    entities::sea_orm_active_enums::{Role},
};

#[derive(Serialize)]
pub struct DeleteKeyResponse {
    message: String,
}

#[utoipa::path(
    delete,
    tags = ["Key"],
    description = "Delete a key by ID",
    path = "/{id}",
    responses(
        (status = 200, description = "Key deleted successfully", body = DeleteKeyResponse),
        (status = 404, description = "Key not found", body = String),
        (status = 500, description = "Failed to delete key", body = String),
    ),
    params(
        ("id" = String, Path, description = "Key ID to delete")
    ),
    security(
        ("session_cookie" = []),
    )
)]
pub async fn delete_key(
    State(state): State<AppState>,
    Path(id): Path<String>,  
) -> impl IntoResponse {
    match key::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(_)) => {
            match key::Entity::delete_by_id(id).exec(&state.db).await {
                Ok(_) => {
                    let response = DeleteKeyResponse {
                        message: "Key deleted successfully".to_string(),
                    };
                    (StatusCode::OK, Json(response)).into_response()
                }
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to delete key".to_string(),
                ).into_response(),
            }
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Key not found".to_string()).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch key".to_string()).into_response(),
    }
}

pub fn key_router() -> Router<AppState> {
    Router::new()
        .route("/:id", delete(delete_key))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
}
