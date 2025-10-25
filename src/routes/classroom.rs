use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use crate::entities::classroom;
use crate::entities::sea_orm_active_enums::ClassroomStatus;
use nanoid::nanoid;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    EntityTrait,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
};

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateClassroomBody {
    name: String,
    capacity: i32,
    location: String,
    room_code: String,
    description: String,
    photo_url: String,
}

#[utoipa::path(
    post,
    tags = ["Classroom"],
    description = "Create new classroom",
    path = "/",
    request_body = CreateClassroomBody,
    responses(
        (status = 201, description = "Classroom created successfully", body = classroom::Model),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_classroom(
    State(state): State<AppState>,
    Json(CreateClassroomBody {
        name,
        capacity,
        location,
        room_code,
        description,
        photo_url,
    }): Json<CreateClassroomBody>,
) -> impl IntoResponse {
    let new_classroom = classroom::ActiveModel {
        id: Set(nanoid!()),
        name: Set(name),
        capacity: Set(capacity),
        location: Set(location),
        status: Set(ClassroomStatus::Available),
        created_at: NotSet,
        updated_at: NotSet,
        room_code: Set(room_code),
        description: Set(description),
        photo_url: Set(photo_url),
    };

    match new_classroom.insert(&state.db).await {
        Ok(classroom) => (StatusCode::CREATED, Json(classroom)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create classroom",
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    tags = ["Classroom"],
    description = "Get list of classroom",
    path = "/",
    responses(
        (status = 200, description = "List of classrooms", body = Vec<classroom::Model>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_classrooms(State(state): State<AppState>) -> impl IntoResponse {
    match classroom::Entity::find().all(&state.db).await {
        Ok(classrooms) => (StatusCode::OK, Json(classrooms)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch classrooms",
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    tags = ["Classroom"],
    description = "Get classroom by ID",
    path = "/{id}",
    params(
        ("id" = String, Path, description = "Classroom ID")
    ),
    responses(
        (status = 200, description = "Classroom found", body = classroom::Model),
        (status = 404, description = "Classroom not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_classroom(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match classroom::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(classroom)) => (StatusCode::OK, Json(classroom)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Classroom not found").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch classroom",
        )
            .into_response(),
    }
}

pub fn classroom_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_classrooms).post(create_classroom))
        .route(
            "/{id}",
            get(get_classroom), // .put(update_classroom)
                                // .delete(delete_classroom),
        )
}
