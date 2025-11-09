use std::sync::{Arc, OnceLock};

use crate::entities::sea_orm_active_enums::{ClassroomStatus, Role};
use crate::{entities::classroom, loginsystem::AuthBackend};
use axum::routing::post;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use axum_login::permission_required;
use axum_typed_multipart::{FieldData, TryFromMultipart, TypedMultipart};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use nanoid::nanoid;
use reqwest::multipart::Part;
use reqwest::{Client, multipart};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    EntityTrait,
};
use utoipa::ToSchema;

use crate::AppState;

static IMGBB_API_KEY: OnceLock<String> = OnceLock::new();
static IMGBB_CLIENT: OnceLock<Arc<Client>> = OnceLock::new();

#[derive(TryFromMultipart, ToSchema)]
pub struct CreateClassroomBody {
    name: String,
    capacity: i32,
    location: String,
    room_code: String,
    description: String,
    #[form_data(limit = "5MB")]
    #[schema(value_type = String, format = "binary")]
    photo: FieldData<Bytes>,
}

#[utoipa::path(
    post,
    tags = ["Classroom"],
    description = "Create new classroom",
    path = "",
    request_body(content = CreateClassroomBody, content_type = "multipart/form-data"),
    responses(
        (status = 201, description = "Classroom created successfully", body = classroom::Model),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn create_classroom(
    State(state): State<AppState>,
    TypedMultipart(CreateClassroomBody {
        name,
        capacity,
        location,
        room_code,
        description,
        photo,
    }): TypedMultipart<CreateClassroomBody>,
) -> impl IntoResponse {
    //TODO: Handle photo upload to storage service
    let key = IMGBB_API_KEY.get().expect("IMGBB_API_KEY not set").clone();
    let client = IMGBB_CLIENT.get().expect("IMGBB_CLIENT not set").clone();

    let body = multipart::Form::new().text("key", key).part(
        "image",
        Part::bytes(photo.contents.to_vec()).file_name(photo.metadata.file_name.unwrap()),
    );

    let response = client
        .post("https://api.imgbb.com/1/upload")
        .multipart(body)
        .send()
        .await;

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
        photo_url: Set("ecw".to_owned()),
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
    path = "",
    responses(
        (status = 200, description = "List of classrooms", body = Vec<classroom::Model>),
        (status = 500, description = "Internal server error", body = String),
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
        (status = 404, description = "Classroom not found", body = String),
        (status = 500, description = "Internal server error", body = String),
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

pub fn classroom_router(imgbb_api_key: String) -> Router<AppState> {
    IMGBB_API_KEY
        .set(imgbb_api_key)
        .expect("IMGBB_API_KEY already set");
    let client_arc = Arc::new(Client::new());
    IMGBB_CLIENT
        .set(client_arc)
        .expect("IMGBB_CLIENT already set");

    let admin_only_route = Router::new()
        .route("/", post(create_classroom))
        .route_layer(permission_required!(AuthBackend, Role::Admin));

    Router::new()
        .route("/", get(list_classrooms))
        .route(
            "/{id}",
            get(get_classroom), // .put(update_classroom)
                                // .delete(delete_classroom),
        )
        .merge(admin_only_route)
}
