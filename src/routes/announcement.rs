use crate::{
    AppState,
    entities::{announcement, sea_orm_active_enums::Role},
    login_system::{AuthBackend, AuthSession},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
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

#[derive(Deserialize, ToSchema)]
pub struct CreateAnnouncementBody {
    pub title: String,
    pub content: String,
}

#[utoipa::path(
    post,
    tags = ["Announcement"],
    description = "Create a new announcement",
    path = "",
    request_body(content = CreateAnnouncementBody, content_type = "application/json"),
    responses(
        (status = 201, description = "Announcement created successfully", body = announcement::Model),
    )
)]
pub async fn create_announcement(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<CreateAnnouncementBody>,
) -> impl IntoResponse {
    let user = match session.user {
        Some(u) => u,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };
    let new_announcement = announcement::ActiveModel {
        id: Set(nanoid!()),
        title: Set(body.title),
        content: Set(body.content),
        published_at: NotSet,
        created_by: Set(Some(user.id)),
    };

    match new_announcement.insert(&state.db).await {
        Ok(announcement) => (StatusCode::CREATED, Json(announcement)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create announcement",
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    tags = ["Announcement"],
    description = "Get all announcements",
    path = "",
    responses(
        (status = 200, description = "Announcements fetched successfully", body = Vec<announcement::Model>),
    )
)]
pub async fn list_announcements(State(state): State<AppState>) -> impl IntoResponse {
    let announcements = match announcement::Entity::find().all(&state.db).await {
        Ok(announcements) => announcements,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch announcements",
            )
                .into_response();
        }
    };
    (StatusCode::OK, Json(announcements)).into_response()
}

#[utoipa::path(
    get,
    tags = ["Announcement"],
    description = "Get announcement by ID",
    path = "/{id}",
    responses(
        (status = 200, description = "Announcement fetched successfully", body = announcement::Model),
    )
)]
pub async fn get_announcement(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let announcement = match announcement::Entity::find_by_id(&id).one(&state.db).await {
        Ok(Some(announcement)) => announcement,
        Ok(None) => return (StatusCode::NOT_FOUND, "Announcement not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch announcement",
            )
                .into_response();
        }
    };
    (StatusCode::OK, Json(announcement)).into_response()
}

#[utoipa::path(
    delete,
    tags = ["Announcement"],
    description = "Delete announcement by ID",
    path = "/{id}",
    responses(
        (status = 200, description = "Announcement deleted successfully"),
    )
)]
pub async fn delete_announcement(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let announcement = match announcement::Entity::find_by_id(&id).one(&state.db).await {
        Ok(Some(announcement)) => announcement,
        Ok(None) => return (StatusCode::NOT_FOUND, "Announcement not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch announcement",
            )
                .into_response();
        }
    };
    match announcement.delete(&state.db).await {
        Ok(_) => (StatusCode::OK, "Announcement deleted successfully").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete announcement").into_response(),
    }
}

pub fn announcement_router() -> Router<AppState> {
    let admin_only_route = Router::new()
        .route("/", post(create_announcement))
        .route("/{id}", delete(delete_announcement))
        .route_layer(permission_required!(AuthBackend, Role::Admin));

    Router::new()
        .route("/", get(list_announcements))
        .route("/{id}", get(get_announcement))
        .merge(admin_only_route)
}
