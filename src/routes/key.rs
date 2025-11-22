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
use crate::{
    AppState,
    entities::key,
    entities::sea_orm_active_enums::{Role, KeyStatus},
};

#[derive(Deserialize)]
pub struct UpdateKeyBody {
    key_number: Option<String>,
    status: Option<KeyStatus>,
    classroom_id: Option<String>,
}

#[derive(Serialize)]
pub struct DeleteKeyResponse {
    message: String,
}

#[derive(Serialize)]
pub struct UpdateKeyResponse {
    message: String,
}

#[utoipa::path(
    put,
    tags = ["Key"],
    description = "Update an existing key by ID",
    path = "/{id}",
    request_body(content = UpdateKeyBody, content_type = "application/json"),
    responses(
        (status = 200, description = "Key updated successfully", body = UpdateKeyResponse),
        (status = 404, description = "Key not found", body = String),
        (status = 500, description = "Failed to update key", body = String),
    ),
    params(
        ("id" = String, Path, description = "Key ID to update")
    ),
    security(
        ("session_cookie" = []),
    )
)]
pub async fn update_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateKeyBody>,
) -> impl IntoResponse {
    let mut key = match key::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(key)) => key,
        Ok(None) => return (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch key").into_response(),
    };

    if let Some(key_number) = body.key_number {
        key.key_number = sea_orm::Set(key_number);
    }
    if let Some(status) = body.status {
        key.status = sea_orm::Set(status);
    }
    if let Some(classroom_id) = body.classroom_id {
        key.classroom_id = sea_orm::Set(classroom_id);
    }

    match key.update(&state.db).await {
        Ok(_) => {
            let response = UpdateKeyResponse {
                message: "Key updated successfully".to_string(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to update key").into_response(),
    }
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
        .route("/:id", put(update_key))
        .route("/:id", delete(delete_key))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
}
