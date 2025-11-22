use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{put, delete},
};
use axum_login::permission_required;
use sea_orm::{ActiveModelTrait, EntityTrait};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    entities::{key, sea_orm_active_enums::Role},
};

#[derive(Deserialize, ToSchema)]
pub struct UpdateKeyBody {
    pub classroom_id: String,
    pub key_number: String,
}

#[utoipa::path(
    put,
    tags = ["Key"],
    description = "Update key information",
    path = "/{id}",
    request_body(content = UpdateKeyBody, content_type = "application/json"),
    responses(
        (status = 200, description = "Key updated successfully", body = key::Model),
        (status = 404, description = "Key not found"),
        (status = 500, description = "Failed to update key"),
    ),
    params(
        ("id" = String, Path, description = "Key ID")
    ),
    security(
        ("session_cookie" = [])
    )
)]
pub async fn update_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateKeyBody>,
) -> impl IntoResponse {
    let UpdateKeyBody {
        classroom_id,
        key_number,
    } = body;

    match key::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(existing_key)) => {
            let mut key: key::ActiveModel = existing_key.into();
            key.classroom_id = Set(Some(classroom_id));
            key.key_number = Set(key_number);

            match key.update(&state.db).await {
                Ok(updated_key) => (StatusCode::OK, Json(updated_key)).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to update key",
                )
                    .into_response(),
            }
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch key for update",
        )
            .into_response(),
    }
}

#[utoipa::path(
    delete,
    tags = ["Key"],
    description = "Delete a key",
    path = "/{id}",
    responses(
        (status = 200, description = "Key deleted successfully"),
        (status = 404, description = "Key not found"),
        (status = 500, description = "Failed to delete key"),
    ),
    params(
        ("id" = String, Path, description = "Key ID")
    ),
    security(
        ("session_cookie" = [])
    )
)]
pub async fn delete_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match key::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(key)) => {
            match key.delete(&state.db).await {
                Ok(_) => (StatusCode::OK, "Key deleted successfully").into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to delete key",
                )
                    .into_response(),
            }
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch key for deletion",
        )
            .into_response(),
    }
}

pub fn key_router() -> Router<AppState> {
    Router::new()
        .route("/{id}", put(update_key).delete(delete_key))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
}
