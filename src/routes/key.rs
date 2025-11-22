use std::sync::{Arc, OnceLock};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
};
use axum_login::permission_required;
use nanoid::nanoid;
use sea_orm::{
    ActiveModelTrait, EntityTrait, ModelTrait,
    ActiveValue::{NotSet, Set},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    loginsystem::AuthBackend,
    entities::{key, classroom},
    entities::sea_orm_active_enums::Role,
};

#[derive(Deserialize, ToSchema)]
pub struct CreateKeyBody {
    pub key_code: String,
    pub classroom_id: String,
    pub note: Option<String>,
}

#[utoipa::path(
    post,
    tags = ["Key"],
    description = "Create a key for a classroom",
    path = "",
    request_body(content = CreateKeyBody, content_type = "application/json"),
    responses(
        (status = 201, description = "Key created successfully", body = key::Model),
        (status = 404, description = "Classroom not found"),
        (status = 400, description = "Key already exists"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_key(
    State(state): State<AppState>,
    Json(body): Json<CreateKeyBody>,
) -> impl IntoResponse {

    let classroom = classroom::Entity::find_by_id(body.classroom_id.clone())
        .one(&state.db)
        .await;

    match classroom {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Classroom not found").into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to query classroom"
            ).into_response();
        }
    }


    let duplicate = key::Entity::find()
        .filter(key::Column::ClassroomId.eq(body.classroom_id.clone()))
        .filter(key::Column::KeyCode.eq(body.key_code.clone()))
        .one(&state.db)
        .await;

    match duplicate {
        Ok(Some(_)) => {
            return (
                StatusCode::BAD_REQUEST,
                "This key already exists for the classroom",
            )
                .into_response();
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
        key_code: Set(body.key_code),
        classroom_id: Set(body.classroom_id),
        note: Set(body.note),
        created_at: NotSet, 
        updated_at: NotSet,
    };

    match new_key.insert(&state.db).await {
        Ok(created) => (StatusCode::CREATED, Json(created)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create key"
        )
            .into_response(),
    }
}



pub fn key_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_key))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
}
