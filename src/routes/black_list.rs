use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use axum_login::permission_required;
use nanoid::nanoid;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    EntityTrait, ModelTrait,
};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    AppState,
    entities::{black_list, sea_orm_active_enums::Role},
    login_system::{AuthBackend, AuthSession},
};

// =========================
//   CREATE BLACKLIST (Admin)
// =========================
#[derive(Deserialize, ToSchema)]
pub struct CreateBlackListBody {
    pub user_id: String,
    pub infraction_id: String,
    pub end_at: Option<String>,
}

#[utoipa::path(
    post,
    tags = ["BlackList"],
    description = "Create a blacklist record",
    path = "",
    request_body(content = CreateBlackListBody, content_type = "application/json"),
    responses(
        (status = 201, description = "Blacklist record created", body = black_list::Model),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Failed to create blacklist record")
    ),
    security(("session_cookie" = []))
)]
pub async fn create_black_list(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<CreateBlackListBody>,
) -> impl IntoResponse {
    let admin = match session.user {
        Some(u) => u,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let end_at_parsed = match body.end_at {
        Some(s) => match s.parse() {
            Ok(dt) => Some(dt),
            Err(_) => return (StatusCode::BAD_REQUEST, "Invalid end_at format").into_response(),
        },
        None => None,
    };

    let new_record = black_list::ActiveModel {
        id: Set(nanoid!()),
        user_id: Set(Some(body.user_id)),
        infraction_id: Set(Some(body.infraction_id)),
        created_by: Set(Some(admin.id)),
        created_at: NotSet,
        end_at: Set(end_at_parsed),
    };

    match new_record.insert(&state.db).await {
        Ok(model) => (StatusCode::CREATED, Json(model)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create blacklist record",
        )
            .into_response(),
    }
}

// =========================
//   RETRIEVE BLACKLIST
// =========================
#[utoipa::path(
    get,
    tags = ["BlackList"],
    description = "Get all blacklist records",
    path = "",
    responses(
        (status = 200, description = "List of blacklist records", body = Vec<black_list::Model>),
        (status = 500, description = "Failed to fetch blacklist records", body = String)
    ),
    security(("session_cookie" = []))
)]
pub async fn list_black_list(State(state): State<AppState>) -> impl IntoResponse {
    match black_list::Entity::find().all(&state.db).await {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch blacklist records",
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    tags = ["BlackList"],
    description = "Get blacklist record by ID",
    path = "/{id}",
    params(("id" = String, Path, description = "Blacklist ID")),
    responses(
        (status = 200, description = "Blacklist record", body = black_list::Model),
        (status = 404, description = "Blacklist record not found", body = String),
        (status = 500, description = "Failed to fetch blacklist record", body = String)
    ),
    security(("session_cookie" = []))
)]
pub async fn get_black_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match black_list::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(model)) => (StatusCode::OK, Json(model)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Blacklist record not found").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch blacklist record",
        )
            .into_response(),
    }
}

// =========================
//   UPDATE BLACKLIST (Admin)
// =========================
#[derive(Deserialize, ToSchema)]
pub struct UpdateBlackListBody {
    pub user_id: Option<String>,
    pub infraction_id: Option<String>,
    pub end_at: Option<String>, // allow updating end_at
}

#[utoipa::path(
    put,
    tags = ["BlackList"],
    description = "Update a blacklist record",
    path = "/{id}",
    request_body(content = UpdateBlackListBody, content_type = "application/json"),
    params(("id" = String, Path, description = "Blacklist ID")),
    responses(
        (status = 200, description = "Blacklist record updated", body = black_list::Model),
        (status = 404, description = "Blacklist record not found", body = String),
        (status = 400, description = "Bad request", body = String),
        (status = 500, description = "Failed to update blacklist record", body = String)
    ),
    security(("session_cookie" = []))
)]
pub async fn update_black_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBlackListBody>,
) -> impl IntoResponse {
    let Some(model) = black_list::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap_or(None)
    else {
        return (StatusCode::NOT_FOUND, "Blacklist record not found").into_response();
    };

    let mut active: black_list::ActiveModel = model.into();

    if let Some(uid) = body.user_id {
        active.user_id = Set(Some(uid));
    }
    if let Some(infraction_id) = body.infraction_id {
        active.infraction_id = Set(Some(infraction_id));
    }
    if let Some(end_at_str) = body.end_at {
        let end_at_parsed = match end_at_str.parse() {
            Ok(dt) => dt,
            Err(_) => return (StatusCode::BAD_REQUEST, "Invalid end_at format").into_response(),
        };
        active.end_at = Set(Some(end_at_parsed));
    }

    match active.update(&state.db).await {
        Ok(updated) => (StatusCode::OK, Json(updated)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update blacklist record",
        )
            .into_response(),
    }
}

// =========================
//   DELETE BLACKLIST (Admin)
// =========================
#[utoipa::path(
    delete,
    tags = ["BlackList"],
    description = "Delete a blacklist record",
    path = "/{id}",
    params(("id" = String, Path, description = "Blacklist ID")),
    responses(
        (status = 200, description = "Blacklist record deleted", body = String),
        (status = 404, description = "Blacklist record not found", body = String),
        (status = 500, description = "Failed to delete blacklist record", body = String)
    ),
    security(("session_cookie" = []))
)]
pub async fn delete_black_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let Some(model) = black_list::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap_or(None)
    else {
        return (StatusCode::NOT_FOUND, "Blacklist record not found").into_response();
    };

    match model.delete(&state.db).await {
        Ok(_) => (StatusCode::OK, "Blacklist record deleted").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to delete blacklist record",
        )
            .into_response(),
    }
}

// =========================
//   ROUTER
// =========================
pub fn black_list_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_black_list))
        .route("/", get(list_black_list))
        .route("/{id}", get(get_black_list))
        .route("/{id}", put(update_black_list))
        .route("/{id}", delete(delete_black_list))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
}
