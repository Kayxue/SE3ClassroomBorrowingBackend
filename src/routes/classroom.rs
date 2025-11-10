use std::sync::{Arc, OnceLock};

use crate::entities::{key, reservation};
use crate::entities::sea_orm_active_enums::{ClassroomStatus, Role};
use crate::{entities::classroom, loginsystem::AuthBackend};
use axum::extract::Query;
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
use nanoid::nanoid;
use reqwest::multipart::Part;
use reqwest::{Client, multipart};
use sea_orm::ModelTrait;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    EntityTrait,
};
use serde::{Deserialize, Serialize};
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

#[derive(Deserialize)]
#[allow(dead_code)]
struct ImgBBResponseData{
    pub status: u16,
    pub success: bool,
    pub data: ImgBBImageData,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ImgBBImageData{
    pub id: String,
    pub title: String,
    pub url_viewer: String,
    pub url: String,
    pub display_url: String,
    pub width: u32,
    pub height: u32,
    pub size: u32,
    pub time: u32,
    pub expiration: u32,
    pub image: ImgBBImageInfo,
    pub thumb: ImgBBImageInfo,
    pub medium: ImgBBImageInfo,
    pub delete_url: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ImgBBImageInfo{
    pub filename: String,
    pub name: String,
    pub mime: String,
    pub extension: String,
    pub url: String,
}

#[derive(Deserialize, ToSchema)]
pub struct GetClassroomQuery{
    with_keys: Option<bool>,
    with_reservations: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct GetClassroomKeyReservationResponse{
    classroom: classroom::Model,
    keys: Vec<key::Model>,
    reservations: Vec<reservation::Model>,
}

#[derive(Serialize, ToSchema)]
pub struct GetClassroomKeyResponse{
    classroom: classroom::Model,
    keys: Vec<key::Model>,
}

#[derive(Serialize, ToSchema)]
pub struct GetClassroomReservationResponse{
    classroom: classroom::Model,
    reservations: Vec<reservation::Model>,
}

#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub enum GetClassroomResponse {
    Classroom(classroom::Model),
    ClassroomWithKeys(GetClassroomKeyResponse),
    ClassroomWithReservations(GetClassroomReservationResponse),
    ClassroomWithKeysAndReservations(GetClassroomKeyReservationResponse),
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
    //TODO: Change to custom photo upload to storage service
    let key = IMGBB_API_KEY.get().expect("IMGBB_API_KEY not set").clone();
    let client = IMGBB_CLIENT.get().expect("IMGBB_CLIENT not set").clone();

    let body = multipart::Form::new().text("key", key).part(
        "image",
        Part::bytes(photo.contents.to_vec()).file_name(photo.metadata.file_name.unwrap()),
    );

    let response = match client
        .post("https://api.imgbb.com/1/upload")
        .multipart(body)
        .send()
        .await
    {
        Ok(resp) => resp.json::<ImgBBResponseData>().await.unwrap(),
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to upload image").into_response();
        }
    };

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
        photo_url: Set(response.data.url),
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
    description = "Get classroom by ID with optional related data. Use query parameters to include keys and/or reservations.",
    path = "/{id}",
    params(
        ("id" = String, Path, description = "Classroom ID"),
        ("with_keys" = Option<bool>, Query, description = "Include related keys in response"),
        ("with_reservations" = Option<bool>, Query, description = "Include related reservations in response")
    ),
    responses(
        (status = 200, description = "Classroom found. Response format varies based on query parameters: \n- No params: Returns classroom object only\n- with_keys=true: Returns ClassroomWithKeys\n- with_reservations=true: Returns ClassroomWithReservations\n- Both params=true: Returns ClassroomWithKeysAndReservations", body = GetClassroomResponse),
        (status = 404, description = "Classroom not found", body = String),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn get_classroom(
    Query(query): Query<GetClassroomQuery>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let GetClassroomQuery {
        with_keys,
        with_reservations,
    } = query;

    match classroom::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(classroom)) => {
            match (with_keys,with_reservations) {
                (Some(true), Some(true)) => {
                    // Fetch keys and reservations separately
                    let keys_result = classroom
                        .find_related(crate::entities::key::Entity)
                        .all(&state.db)
                        .await;
                    
                    let reservations_result = classroom
                        .find_related(crate::entities::reservation::Entity)
                        .all(&state.db)
                        .await;
                    
                    match (keys_result, reservations_result) {
                        (Ok(keys), Ok(reservations)) => {
                            // Combine the results
                            let response = serde_json::json!({
                                "classroom": classroom,
                                "keys": keys,
                                "reservations": reservations,
                            });
                            (StatusCode::OK, Json(response)).into_response()
                        },
                        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch classroom with keys and reservations").into_response(),
                    }
                },
                (Some(true), _) => {
                    let keys_result = classroom
                        .find_related(crate::entities::key::Entity)
                        .all(&state.db)
                        .await;
                    match keys_result {
                        Ok(keys) => {
                            let response = serde_json::json!({
                                "classroom": classroom,
                                "keys": keys,
                            });
                            (StatusCode::OK, Json(response)).into_response()
                        },
                        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch classroom with keys").into_response(),
                    }
                },
                (_, Some(true)) => {
                    let reservations_result = classroom
                        .find_related(crate::entities::reservation::Entity)
                        .all(&state.db)
                        .await;
                    match reservations_result {
                        Ok(reservations) => {
                            let response = serde_json::json!({
                                "classroom": classroom,
                                "reservations": reservations,
                            });
                            (StatusCode::OK, Json(response)).into_response()
                        },
                        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch classroom with reservations").into_response(),
                    }
                },
                _ => (StatusCode::OK, Json(classroom)).into_response(),
            }
        },
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
