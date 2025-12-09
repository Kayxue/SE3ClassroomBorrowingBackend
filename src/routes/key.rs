use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, post, put},
};
use axum_login::permission_required;
use nanoid::nanoid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, ModelTrait, QueryFilter,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    entities::sea_orm_active_enums::Role,
    entities::{classroom, key},
    login_system::AuthBackend,
};

#[derive(Deserialize, ToSchema)]
pub struct CreateKeyBody {
    pub key_number: String,
    pub classroom_id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateKeyBody {
    pub key_number: String,
    pub classroom_id: String,
    pub is_active: bool,
}

#[derive(Serialize, ToSchema)]
pub struct KeyResponse {
    pub id: String,
    pub key_number: String,
    pub classroom_id: Option<String>,
    pub is_active: bool,
}

impl From<key::Model> for KeyResponse {
    fn from(model: key::Model) -> Self {
        Self {
            id: model.id,
            key_number: model.key_number,
            classroom_id: model.classroom_id,
            is_active: model.is_active,
        }
    }
}

#[utoipa::path(
    post,
    tags = ["Key"],
    description = "Create a new key assigned to a classroom",
    path = "",
    request_body(content = CreateKeyBody, content_type = "application/json"),
    responses(
        (status = 201, description = "Key created successfully", body = KeyResponse),
        (status = 404, description = "Classroom not found"),
        (status = 400, description = "Key number already exists"),
        (status = 500, description = "Failed to create key")
    )
)]
pub async fn create_key(
    State(state): State<AppState>,
    Json(body): Json<CreateKeyBody>,
) -> impl IntoResponse {
    match classroom::Entity::find_by_id(&body.classroom_id)
        .one(&state.db)
        .await
    {
        Ok(Some(_)) => {}
        Ok(None) => return (StatusCode::NOT_FOUND, "Classroom not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to query classroom",
            )
                .into_response();
        }
    }

    match key::Entity::find()
        .filter(key::Column::KeyNumber.eq(&body.key_number))
        .one(&state.db)
        .await
    {
        Ok(Some(_)) => {
            return (StatusCode::BAD_REQUEST, "This key_number already exists").into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to check key duplication",
            )
                .into_response();
        }
        _ => {}
    }

    let new_key = key::ActiveModel {
        id: Set(nanoid!()),
        key_number: Set(body.key_number),
        classroom_id: Set(Some(body.classroom_id)),
        is_active: Set(true),
    };

    match new_key.insert(&state.db).await {
        Ok(model) => {
            let resp = KeyResponse::from(model);
            (StatusCode::CREATED, Json(resp)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create key").into_response(),
    }
}

#[utoipa::path(
    put,
    tags = ["Key"],
    description = "Update an existing key",
    path = "/{id}",
    request_body(content = UpdateKeyBody, content_type = "application/json"),
    params(
        ("id" = String, Path, description = "Key ID")
    ),
    responses(
        (status = 200, description = "Key updated successfully", body = KeyResponse),
        (status = 404, description = "Key or classroom not found"),
        (status = 400, description = "Key number already exists"),
        (status = 500, description = "Failed to update key")
    )
)]
pub async fn update_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateKeyBody>,
) -> impl IntoResponse {
    let key_model = match key::Entity::find_by_id(&id).one(&state.db).await {
        Ok(Some(k)) => k,
        Ok(None) => return (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch key").into_response();
        }
    };

    match classroom::Entity::find_by_id(&body.classroom_id)
        .one(&state.db)
        .await
    {
        Ok(Some(_)) => {}
        Ok(None) => return (StatusCode::NOT_FOUND, "Classroom not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to query classroom",
            )
                .into_response();
        }
    }

    match key::Entity::find()
        .filter(key::Column::KeyNumber.eq(&body.key_number))
        .filter(key::Column::Id.ne(id.clone()))
        .one(&state.db)
        .await
    {
        Ok(Some(_)) => {
            return (StatusCode::BAD_REQUEST, "This key_number already exists").into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to check key duplication",
            )
                .into_response();
        }
        _ => {}
    }

    let mut key_active: key::ActiveModel = key_model.into();
    key_active.key_number = Set(body.key_number);
    key_active.classroom_id = Set(Some(body.classroom_id));
    key_active.is_active = Set(body.is_active);

    match key_active.update(&state.db).await {
        Ok(updated) => {
            let resp = KeyResponse::from(updated);
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to update key").into_response(),
    }
}

#[utoipa::path(
    delete,
    tags = ["Key"],
    description = "Delete a key",
    path = "/{id}",
    params(
        ("id" = String, Path, description = "Key ID")
    ),
    responses(
        (status = 200, description = "Key deleted successfully"),
        (status = 404, description = "Key not found"),
        (status = 500, description = "Failed to delete key")
    )
)]
pub async fn delete_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let key_model = match key::Entity::find_by_id(&id).one(&state.db).await {
        Ok(Some(k)) => k,
        Ok(None) => return (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch key").into_response();
        }
    };

    match key_model.delete(&state.db).await {
        Ok(_) => (StatusCode::OK, "Key deleted successfully").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete key").into_response(),
    }
}

pub fn key_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_key))
        .route("/{id}", put(update_key))
        .route("/{id}", delete(delete_key))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
}
