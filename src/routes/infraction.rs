use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use axum_login::{login_required, permission_required};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    ColumnTrait, EntityTrait, ModelTrait, QueryFilter,
};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    AppState,
    entities::{infraction, sea_orm_active_enums::Role},
    login_system::{AuthBackend, AuthSession},
};
use nanoid::nanoid;

#[derive(Deserialize, ToSchema)]
pub struct CreateInfractionBody {
    pub user_id: String,
    pub reservation_id: String,
    pub description: String,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateInfractionBody {
    pub description: String,
}

#[utoipa::path(
    post,
    tags = ["Infraction"],
    description = "Create a new infraction",
    path = "",
    request_body(content = CreateInfractionBody, content_type = "application/json"),
    responses(
        (status = 201, description = "Infraction created successfully", body = infraction::Model),
    )
)]
pub async fn create_infraction(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<CreateInfractionBody>,
) -> impl IntoResponse {
    let user = session.user.unwrap();
    let new_infraction = infraction::ActiveModel {
        id: Set(nanoid!()),
        user_id: Set(Some(body.user_id)),
        reservation_id: Set(Some(body.reservation_id)),
        description: Set(body.description),
        created_by: Set(Some(user.id)),
        created_at: NotSet,
    };
    match new_infraction.insert(&state.db).await {
        Ok(infraction) => (StatusCode::CREATED, Json(infraction)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create infraction",
        )
            .into_response(),
    }
}

#[utoipa::path(
    put,
    tags = ["Infraction"],
    description = "Update an infraction",
    path = "/{id}",
    request_body(content = UpdateInfractionBody, content_type = "application/json"),
    responses(
        (status = 200, description = "Infraction updated successfully", body = infraction::Model),
    )
)]
pub async fn update_infraction(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateInfractionBody>,
) -> impl IntoResponse {
    let infraction = match infraction::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(infraction)) => infraction,
        Ok(None) => return (StatusCode::NOT_FOUND, "Infraction not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch infraction",
            )
                .into_response();
        }
    };
    let mut updated_infraction: infraction::ActiveModel = infraction.into();
    updated_infraction.description = Set(body.description);
    match updated_infraction.update(&state.db).await {
        Ok(infraction) => (StatusCode::OK, Json(infraction)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update infraction",
        )
            .into_response(),
    }
}

#[utoipa::path(
    delete,
    tags = ["Infraction"],
    description = "Delete an infraction",
    path = "/{id}",
    responses(
        (status = 200, description = "Infraction deleted successfully"),
    )
)]
pub async fn delete_infraction(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let infraction = match infraction::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(infraction)) => infraction,
        Ok(None) => return (StatusCode::NOT_FOUND, "Infraction not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch infraction",
            )
                .into_response();
        }
    };
    match infraction.delete(&state.db).await {
        Ok(_) => (StatusCode::OK, "Infraction deleted successfully").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to delete infraction",
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    tags = ["Infraction"],
    description = "Get an infraction",
    path = "/{id}",
    responses(
        (status = 200, description = "Infraction fetched successfully", body = infraction::Model),
    )
)]
pub async fn get_infraction(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let infraction = match infraction::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(infraction)) => infraction,
        Ok(None) => return (StatusCode::NOT_FOUND, "Infraction not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch infraction",
            )
                .into_response();
        }
    };
    (StatusCode::OK, Json(infraction)).into_response()
}

#[utoipa::path(
    get,
    tags = ["Infraction"],
    description = "Get all infractions for self",
    path = "",
    responses(
        (status = 200, description = "Infractions fetched successfully", body = Vec<infraction::Model>),
    )
)]
pub async fn list_infractions(
    session: AuthSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user = session.user.unwrap();
    let infractions = match infraction::Entity::find()
        .filter(infraction::Column::UserId.eq(user.id))
        .all(&state.db)
        .await
    {
        Ok(infractions) => infractions,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch infractions",
            )
                .into_response();
        }
    };
    (StatusCode::OK, Json(infractions)).into_response()
}

pub fn infraction_router() -> Router<AppState> {
    let admin_only_route = Router::new()
        .route("/", post(create_infraction))
        .route("/{id}", put(update_infraction))
        .route("/{id}", delete(delete_infraction))
        .route_layer(permission_required!(AuthBackend, Role::Admin));

    let login_required_route = Router::new()
        .route("/", get(list_infractions))
        .route("/{id}", get(get_infraction))
        .route_layer(login_required!(AuthBackend));

    Router::new()
        .merge(admin_only_route)
        .merge(login_required_route)
}
