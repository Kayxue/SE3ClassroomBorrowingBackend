use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
};
use axum_login::permission_required;
use nanoid::nanoid;
use sea_orm::{
    ActiveModelTrait, EntityTrait, QueryFilter, ColumnTrait,
    ActiveValue::Set,
};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    AppState,
    loginsystem::AuthBackend,
    entities::{key, classroom},
    entities::sea_orm_active_enums::Role,
};

#[derive(Deserialize, ToSchema)]
pub struct CreateKeyBody {
    pub key_number: String,
    pub classroom_id: String,
}

#[utoipa::path(
    post,
    tags = ["Key"],
    description = "Create a new key assigned to a classroom",
    path = "",
    request_body(content = CreateKeyBody, content_type = "application/json"),
    responses(
        (status = 201, description = "Key created successfully", body = key::Model),
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
        Err(_) => return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to query classroom"
        ).into_response(),
    }

    match key::Entity::find()
        .filter(key::Column::KeyNumber.eq(&body.key_number))
        .one(&state.db)
        .await
    {
        Ok(Some(_)) => {
            return (
                StatusCode::BAD_REQUEST,
                "This key_number already exists"
            ).into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to check key duplication"
            ).into_response();
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
        Ok(model) => (StatusCode::CREATED, Json(model)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create key"
        ).into_response(),
    }
}

pub fn key_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_key))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
}

