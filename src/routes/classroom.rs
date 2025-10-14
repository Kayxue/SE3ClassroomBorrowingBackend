use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use entities::classroom;
use nanoid::nanoid;
use sea_orm::{ActiveModelTrait, ActiveValue::{NotSet, Set}, EntityTrait};

use crate::{entities::{self, sea_orm_active_enums::Status}, AppState};

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

async fn create_classroom(State(state): State<AppState>) -> impl IntoResponse {
    let new_classroom = classroom::ActiveModel {
        id: Set(nanoid!()),
        name: Set("New Classroom".to_string()),
        capacity: Set(30),
        location: Set("電資大樓二樓".into()),
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

pub fn classroom_router() -> Router<AppState> {
    Router::new()
        .route("/classrooms", get(list_classrooms).post(create_classroom))
        // .route(
        //     "/classrooms/:id",
        //     get(get_classroom)
        //         .put(update_classroom)
        //         .delete(delete_classroom),
        // )
}
