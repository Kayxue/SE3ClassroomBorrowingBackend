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
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    ColumnTrait, EntityTrait, ModelTrait, QueryFilter,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    entities::{classroom, key, key_transaction_log, reservation, sea_orm_active_enums::Role},
    login_system::{AuthBackend, AuthSession},
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

#[derive(Deserialize, ToSchema)]
pub struct BorrowKeyBody {
    pub reservation_id: String,
    pub borrowed_at: String,
    pub deadline: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ReturnKeyBody {
    pub returned_at: String,
    pub on_time: Option<bool>,
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

#[utoipa::path(
    post,
    tags = ["Key"],
    description = "Borrow a key",
    path = "/{id}/borrow",
    request_body(content = BorrowKeyBody, content_type = "application/json"),
    params(
        ("id" = String, Path, description = "Key ID")
    ),
    responses(
        (status = 200, description = "Key borrowed successfully"),
        (status = 404, description = "Key or reservation not found"),
        (status = 400, description = "Key is not active"),
        (status = 500, description = "Failed to borrow key")
    ),
    security(("session_cookie" = []))
)]
pub async fn borrow_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: AuthSession,
    Json(body): Json<BorrowKeyBody>,
) -> impl IntoResponse {
    let key_model = match key::Entity::find_by_id(&id).one(&state.db).await {
        Ok(Some(k)) => k,
        Ok(None) => return (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch key").into_response();
        }
    };

    if !key_model.is_active {
        return (StatusCode::BAD_REQUEST, "Key is not active").into_response();
    }

    let reservation_model = match reservation::Entity::find_by_id(&body.reservation_id)
        .one(&state.db)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, "Reservation not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch reservation",
            )
                .into_response();
        }
    };

    let new_key_transaction_log = key_transaction_log::ActiveModel {
        id: Set(nanoid!()),
        reservation_id: Set(Some(body.reservation_id)),
        key_id: Set(Some(id)),
        borrowed_to: Set(Some(reservation_model.user_id.unwrap())),
        handled_by: Set(Some(session.user.unwrap().id)),
        borrowed_at: Set(body.borrowed_at.parse().unwrap()),
        deadline: Set(body.deadline.parse().unwrap()),
        returned_at: NotSet,
        on_time: NotSet,
        created_at: NotSet,
    };

    match new_key_transaction_log.insert(&state.db).await {
        Ok(_) => (StatusCode::OK, "Key borrowed successfully").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to borrow key").into_response(),
    }
}

#[utoipa::path(
    post,
    tags = ["Key"],
    description = "Return a key",
    path = "/{id}/return",
    request_body(content = ReturnKeyBody, content_type = "application/json"),
    params(
        ("id" = String, Path, description = "Key Transaction Log ID")
    ),
    responses(
        (status = 200, description = "Key returned successfully"),
        (status = 404, description = "Key transaction log not found"),
        (status = 500, description = "Failed to return key")
    ),
    security(("session_cookie" = []))
)]
pub async fn return_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ReturnKeyBody>,
) -> impl IntoResponse {
    let key_transaction_log_model = match key_transaction_log::Entity::find_by_id(&id)
        .one(&state.db)
        .await
    {
        Ok(Some(k)) => k,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Key transaction log not found").into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch key transaction log",
            )
                .into_response();
        }
    };

    if key_transaction_log_model.returned_at.is_some() {
        return (StatusCode::BAD_REQUEST, "Key already returned").into_response();
    }

    let deadline = key_transaction_log_model.deadline;
    let returned_at_parsed = body.returned_at.parse().unwrap();

    let mut key_transaction_log_active: key_transaction_log::ActiveModel =
        key_transaction_log_model.into();
    key_transaction_log_active.returned_at = Set(Some(returned_at_parsed));
    key_transaction_log_active.on_time = Set(body
        .on_time
        .unwrap_or_else(|| returned_at_parsed <= deadline));

    match key_transaction_log_active.update(&state.db).await {
        Ok(_) => (StatusCode::OK, "Key returned successfully").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to return key").into_response(),
    }
}

pub fn key_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_key))
        .route("/{id}", put(update_key))
        .route("/{id}", delete(delete_key))
        .route("/{id}/borrow", post(borrow_key))
        .route("/{id}/return", post(return_key))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
}
