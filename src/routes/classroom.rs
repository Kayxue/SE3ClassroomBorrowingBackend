use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use entities::classroom;
use nanoid::nanoid;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    EntityTrait,
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    entities::{self, sea_orm_active_enums::Status},
};

#[derive(Deserialize, Serialize)]
struct CreateClassroomBody {
    name: String,
    capacity: i32,
    location: String,
}

async fn create_classroom(
    State(state): State<AppState>,
    Json(CreateClassroomBody {
        name,
        capacity,
        location,
    }): Json<CreateClassroomBody>,
) -> impl IntoResponse {
    let new_classroom = classroom::ActiveModel {
        id: Set(nanoid!()),
        name: Set(name),
        capacity: Set(capacity),
        location: Set(location),
        status: Set(Status::Available),
        created_at: NotSet,
        updated_at: NotSet,
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

async fn list_classrooms(State(state): State<AppState>) -> impl IntoResponse {
    match classroom::Entity::find().all(&state.db).await {
        Ok(classrooms) => (StatusCode::OK, Json(classrooms)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch classrooms",
        )
            .into_response(),
    }
}

async fn get_classroom(
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
    Router::new().route("/", get(list_classrooms).post(create_classroom))
    .route(
        "/{id}",
        get(get_classroom)
            // .put(update_classroom)
            // .delete(delete_classroom),
    )
}
