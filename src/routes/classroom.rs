use std::sync::{Arc, OnceLock};

use crate::entities::sea_orm_active_enums::{ClassroomStatus, Role};
use crate::entities::{key, reservation};
use crate::{entities::classroom, login_system::AuthBackend};
use axum::extract::Query;
use axum::routing::{delete, post, put};
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
use redis::AsyncCommands;
use reqwest::multipart::Part;
use reqwest::{Client, multipart};
use sea_orm::ModelTrait;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    EntityTrait,
};
use serde::{Deserialize, Serialize};
use tracing::warn;
use utoipa::ToSchema;

use crate::{AppState, utils::{get_redis_options, REDIS_EXPIRY}};

// Redis cache key helpers
fn classroom_key(id: &str) -> String {
    format!("classroom_{}", id)
}

fn classroom_with_keys_key(id: &str) -> String {
    format!("classroom_{}_keys", id)
}

fn classroom_with_reservations_key(id: &str) -> String {
    format!("classroom_{}_reservations", id)
}

fn classroom_with_keys_and_reservations_key(id: &str) -> String {
    format!("classroom_{}_keys_reservations", id)
}

const CLASSROOMS_LIST_KEY: &str = "classrooms:list";

static IMAGE_SERVICE_API_KEY: OnceLock<String> = OnceLock::new();
static IMAGE_SERVICE_IP: OnceLock<String> = OnceLock::new();
static IMAGE_SERVICE_CLIENT: OnceLock<Arc<Client>> = OnceLock::new();

#[derive(TryFromMultipart, ToSchema)]
pub struct CreateClassroomBody {
    name: String,
    capacity: i32,
    location: String,
    description: String,
    #[form_data(limit = "5MB")]
    #[schema(value_type = String, format = "binary")]
    photo: FieldData<Bytes>,
}

#[derive(Deserialize, ToSchema)]
pub struct GetClassroomQuery {
    with_keys: Option<bool>,
    with_reservations: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateClassroomBody {
    name: String,
    capacity: i32,
    location: String,
    description: String,
}

#[derive(TryFromMultipart, ToSchema)]
pub struct UpdateClassroomPhotoBody {
    #[form_data(limit = "5MB")]
    #[schema(value_type = String, format = "binary")]
    photo: FieldData<Bytes>,
}

#[derive(Serialize, ToSchema)]
pub struct GetClassroomKeyReservationResponse {
    classroom: classroom::Model,
    keys: Vec<key::Model>,
    reservations: Vec<reservation::Model>,
}

#[derive(Serialize, ToSchema)]
pub struct GetClassroomKeyResponse {
    classroom: classroom::Model,
    keys: Vec<key::Model>,
}

#[derive(Serialize, ToSchema)]
pub struct GetClassroomReservationResponse {
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
        description,
        photo,
    }): TypedMultipart<CreateClassroomBody>,
) -> impl IntoResponse {
    let url = IMAGE_SERVICE_IP
        .get()
        .expect("IMAGE_SERVICE_IP not set")
        .clone();
    let key = IMAGE_SERVICE_API_KEY
        .get()
        .expect("IMAGE_SERVICE_API_KEY not set")
        .clone();
    let client = IMAGE_SERVICE_CLIENT
        .get()
        .expect("IMAGE_SERVICE_CLIENT not set")
        .clone();

    let body = multipart::Form::new().part(
        "image",
        Part::bytes(photo.contents.to_vec()).file_name(photo.metadata.file_name.unwrap()),
    );

    let response = match client
        .post(format!("{}/", url))
        .multipart(body)
        .header("key", key)
        .send()
        .await
    {
        Ok(resp) => match resp.status() {
            StatusCode::CREATED => resp.text().await.unwrap(),
            _ => {
                return (StatusCode::BAD_REQUEST, resp.text().await.unwrap()).into_response();
            }
        },
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
        description: Set(description),
        photo_id: Set(response),
    };

    match new_classroom.insert(&state.db).await {
        Ok(classroom) => {
            // Cache the new classroom
            let mut redis = state.redis.clone();
            let result: Result<(), redis::RedisError> = redis
                .set_options(
                    classroom_key(&classroom.id),
                    serde_json::to_string(&classroom).unwrap(),
                    get_redis_options(),
                )
                .await;
            if let Err(e) = result {
                warn!("Failed to cache classroom {} in Redis: {}", classroom.id, e);
            }
            // Invalidate classrooms list cache
            let _: Result<(), redis::RedisError> = redis.del(CLASSROOMS_LIST_KEY).await;
            
            (StatusCode::CREATED, Json(classroom)).into_response()
        }
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
    // Clone connection once for this handler
    let mut redis = state.redis.clone();
    
    // Try to get from cache first
    let cached_classrooms: Option<String> = match redis
        .get_ex(CLASSROOMS_LIST_KEY, REDIS_EXPIRY)
        .await
    {
        Ok(classrooms) => classrooms,
        Err(e) => {
            warn!("Failed to get classrooms list from Redis cache: {}", e);
            None
        }
    };
    
    if let Some(classrooms_str) = cached_classrooms {
        if let Ok(classrooms) = serde_json::from_str::<Vec<classroom::Model>>(&classrooms_str) {
            return (StatusCode::OK, Json(classrooms)).into_response();
        }
    }

    // Fallback to database
    match classroom::Entity::find().all(&state.db).await {
        Ok(classrooms) => {
            // Cache the result for future requests
            let result: Result<(), redis::RedisError> = redis
                .set_options(
                    CLASSROOMS_LIST_KEY,
                    serde_json::to_string(&classrooms).unwrap(),
                    get_redis_options(),
                )
                .await;
            if let Err(e) = result {
                warn!("Failed to cache classrooms list in Redis: {}", e);
            }
            (StatusCode::OK, Json(classrooms)).into_response()
        }
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
    description = "Get classroom by ID with optional related data.",
    path = "/{id}",
    params(
        ("id" = String, Path, description = "Classroom ID"),
        ("with_keys" = Option<bool>, Query),
        ("with_reservations" = Option<bool>, Query)
    ),
    responses(
        (status = 200, body = GetClassroomResponse),
        (status = 404, description = "Classroom not found"),
        (status = 500, description = "Internal server error"),
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

    // Clone connection once for this handler
    let mut redis = state.redis.clone();
    
    // Determine cache key based on query parameters
    let cache_key = match (with_keys, with_reservations) {
        (Some(true), Some(true)) => classroom_with_keys_and_reservations_key(&id),
        (Some(true), _) => classroom_with_keys_key(&id),
        (_, Some(true)) => classroom_with_reservations_key(&id),
        _ => classroom_key(&id),
    };

    // Try to get from cache first
    let cached_data: Option<String> = match redis.get_ex(&cache_key, REDIS_EXPIRY).await {
        Ok(data) => data,
        Err(e) => {
            warn!("Failed to get classroom {} from Redis cache: {}", id, e);
            None
        }
    };

    if let Some(data_str) = cached_data {
        // Try to parse as the appropriate response type
        if let Ok(response) = serde_json::from_str::<serde_json::Value>(&data_str) {
            return (StatusCode::OK, Json(response)).into_response();
        }
    }

    // Fallback to database
    match classroom::Entity::find_by_id(id.clone()).one(&state.db).await {
        Ok(Some(classroom)) => {
            match (with_keys, with_reservations) {
                (Some(true), Some(true)) => {
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
                            let response = serde_json::json!({
                                "classroom": classroom,
                                "keys": keys,
                                "reservations": reservations,
                            });
                            // Cache the response
                            let _: Result<(), redis::RedisError> = redis
                                .set_options(
                                    &cache_key,
                                    serde_json::to_string(&response).unwrap(),
                                    get_redis_options(),
                                )
                                .await;
                            return (StatusCode::OK, Json(response)).into_response();
                        }
                        _ => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Failed to fetch classroom with keys and reservations",
                            )
                                .into_response();
                        }
                    }
                }
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
                            // Cache the response
                            let _: Result<(), redis::RedisError> = redis
                                .set_options(
                                    &cache_key,
                                    serde_json::to_string(&response).unwrap(),
                                    get_redis_options(),
                                )
                                .await;
                            return (StatusCode::OK, Json(response)).into_response();
                        }
                        Err(_) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Failed to fetch classroom with keys",
                            )
                                .into_response();
                        }
                    }
                }
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
                            // Cache the response
                            let _: Result<(), redis::RedisError> = redis
                                .set_options(
                                    &cache_key,
                                    serde_json::to_string(&response).unwrap(),
                                    get_redis_options(),
                                )
                                .await;
                            return (StatusCode::OK, Json(response)).into_response();
                        }
                        Err(_) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Failed to fetch classroom with reservations",
                            )
                                .into_response();
                        }
                    }
                }
                _ => {
                    // Cache the basic classroom
                    let result: Result<(), redis::RedisError> = redis
                        .set_options(
                            &cache_key,
                            serde_json::to_string(&classroom).unwrap(),
                            get_redis_options(),
                        )
                        .await;
                    if let Err(e) = result {
                        warn!("Failed to cache classroom {} in Redis: {}", id, e);
                    }
                    (StatusCode::OK, Json(classroom)).into_response()
                }
            }
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Classroom not found").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch classroom",
        )
            .into_response(),
    }
}

// =========================
//   UPDATE CLASSROOM
// =========================

#[utoipa::path(
    put,
    tags = ["Classroom"],
    description = "Update classroom",
    path = "/{id}",
    request_body(content = UpdateClassroomBody, content_type = "application/json"),
    responses(
        (status = 200, description = "Classroom updated successfully", body = classroom::Model),
        (status = 404, description = "Classroom not found"),
        (status = 500, description = "Failed to update classroom")
    )
)]
pub async fn update_classroom(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateClassroomBody>,
) -> impl IntoResponse {
    match classroom::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(classroom_model)) => {
            let mut classroom: classroom::ActiveModel = classroom_model.into();

            classroom.name = Set(body.name);
            classroom.capacity = Set(body.capacity);
            classroom.location = Set(body.location);
            classroom.description = Set(body.description);

            match classroom.update(&state.db).await {
                Ok(updated) => {
                    // Update cache and invalidate related caches
                    let mut redis = state.redis.clone();
                    let result: Result<(), redis::RedisError> = redis
                        .set_options(
                            classroom_key(&updated.id),
                            serde_json::to_string(&updated).unwrap(),
                            get_redis_options(),
                        )
                        .await;
                    if let Err(e) = result {
                        warn!("Failed to update cache for classroom {} in Redis: {}", updated.id, e);
                    }
                    // Invalidate all related caches for this classroom
                    let _: Result<(), redis::RedisError> = redis.del(classroom_with_keys_key(&updated.id)).await;
                    let _: Result<(), redis::RedisError> = redis.del(classroom_with_reservations_key(&updated.id)).await;
                    let _: Result<(), redis::RedisError> = redis.del(classroom_with_keys_and_reservations_key(&updated.id)).await;
                    // Invalidate classrooms list cache
                    let _: Result<(), redis::RedisError> = redis.del(CLASSROOMS_LIST_KEY).await;
                    
                    (StatusCode::OK, Json(updated)).into_response()
                }
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to update classroom",
                )
                    .into_response(),
            }
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Classroom not found").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update classroom",
        )
            .into_response(),
    }
}

// =========================
//   UPDATE CLASSROOM PHOTO
// =========================

#[utoipa::path(
    put,
    tags = ["Classroom"],
    description = "Update classroom photo",
    path = "/{id}/photo",
    request_body(
        content = UpdateClassroomPhotoBody,
        content_type = "multipart/form-data"
    ),
    params(
        ("id" = String, Path, description = "Classroom ID")
    ),
    responses(
        (status = 200, description = "Photo updated successfully", body = classroom::Model),
        (status = 404, description = "Classroom not found"),
        (status = 500, description = "Failed to update classroom photo")
    )
)]
pub async fn update_classroom_photo(
    State(state): State<AppState>,
    Path(id): Path<String>,
    TypedMultipart(UpdateClassroomPhotoBody { photo }): TypedMultipart<UpdateClassroomPhotoBody>,
) -> impl IntoResponse {
    let Some(classroom_model) = classroom::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap_or(None)
    else {
        return (StatusCode::NOT_FOUND, "Classroom not found").into_response();
    };

    let current_photo_id = &classroom_model.photo_id;

    let base_url = IMAGE_SERVICE_IP.get().unwrap().clone();
    let key = IMAGE_SERVICE_API_KEY.get().unwrap().clone();
    let client = IMAGE_SERVICE_CLIENT.get().unwrap().clone();

    let form = multipart::Form::new().part(
        "image",
        Part::bytes(photo.contents.to_vec()).file_name(photo.metadata.file_name.unwrap()),
    );

    let url = format!("{}/{}", base_url, current_photo_id);

    let upload_result = client
        .put(url)
        .multipart(form)
        .header("key", key)
        .send()
        .await;

    match upload_result {
        Ok(resp) => {
            if resp.status().is_success() {
                // Update cache and invalidate related caches
                let mut redis = state.redis.clone();
                let result: Result<(), redis::RedisError> = redis
                    .set_options(
                        classroom_key(&classroom_model.id),
                        serde_json::to_string(&classroom_model).unwrap(),
                        get_redis_options(),
                    )
                    .await;
                if let Err(e) = result {
                    warn!("Failed to update cache for classroom {} in Redis: {}", classroom_model.id, e);
                }
                // Invalidate all related caches for this classroom
                let _: Result<(), redis::RedisError> = redis.del(classroom_with_keys_key(&classroom_model.id)).await;
                let _: Result<(), redis::RedisError> = redis.del(classroom_with_reservations_key(&classroom_model.id)).await;
                let _: Result<(), redis::RedisError> = redis.del(classroom_with_keys_and_reservations_key(&classroom_model.id)).await;
                // Invalidate classrooms list cache
                let _: Result<(), redis::RedisError> = redis.del(CLASSROOMS_LIST_KEY).await;
                
                (StatusCode::OK, Json(classroom_model)).into_response()
            } else {
                (StatusCode::BAD_REQUEST, resp.text().await.unwrap()).into_response()
            }
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to upload new photo",
        )
            .into_response(),
    }
}

// =========================
//   DELETE CLASSROOM
// =========================

#[utoipa::path(
    delete,
    tags = ["Classroom"],
    description = "Delete classroom",
    path = "/{id}",
    responses(
        (status = 200, description = "Classroom deleted successfully"),
        (status = 404, description = "Classroom not found"),
        (status = 500, description = "Failed to delete classroom")
    )
)]
pub async fn delete_classroom(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let classroom_model = match classroom::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(c)) => c,
        Ok(None) => return (StatusCode::NOT_FOUND, "Classroom not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch classroom",
            )
                .into_response();
        }
    };

    let photo_id = &classroom_model.photo_id;

    let base_url = IMAGE_SERVICE_IP.get().unwrap().clone();
    let key = IMAGE_SERVICE_API_KEY.get().unwrap().clone();
    let client = IMAGE_SERVICE_CLIENT.get().unwrap().clone();

    let image_delete_url = format!("{}/{}", base_url, photo_id);

    let delete_image_result = client
        .delete(image_delete_url)
        .header("key", key)
        .send()
        .await;

    if delete_image_result.is_err() {
        println!("WARN: Failed to delete classroom image on image server.");
    }

    // Save classroom ID before deleting (delete consumes the model)
    let classroom_id = classroom_model.id.clone();
    
    match classroom_model.delete(&state.db).await {
        Ok(_) => {
            // Invalidate all caches for this classroom
            let mut redis = state.redis.clone();
            let _: Result<(), redis::RedisError> = redis.del(classroom_key(&classroom_id)).await;
            let _: Result<(), redis::RedisError> = redis.del(classroom_with_keys_key(&classroom_id)).await;
            let _: Result<(), redis::RedisError> = redis.del(classroom_with_reservations_key(&classroom_id)).await;
            let _: Result<(), redis::RedisError> = redis.del(classroom_with_keys_and_reservations_key(&classroom_id)).await;
            // Invalidate classrooms list cache
            let _: Result<(), redis::RedisError> = redis.del(CLASSROOMS_LIST_KEY).await;
            
            (StatusCode::OK, "Classroom deleted successfully").into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to delete classroom",
        )
            .into_response(),
    }
}

pub fn classroom_router(
    image_service_url: String,
    image_service_api_key: String,
) -> Router<AppState> {
    IMAGE_SERVICE_IP
        .set(image_service_url)
        .expect("IMAGE_SERVICE_IP already set");
    IMAGE_SERVICE_API_KEY
        .set(image_service_api_key)
        .expect("IMAGE_SERVICE_API_KEY already set");
    let client_arc = Arc::new(Client::new());
    IMAGE_SERVICE_CLIENT
        .set(client_arc)
        .expect("IMAGE_SERVICE_CLIENT already set");

    let admin_only_route = Router::new()
        .route("/", post(create_classroom))
        .route("/{id}", put(update_classroom))
        .route("/{id}/photo", put(update_classroom_photo))
        .route("/{id}", delete(delete_classroom))
        .route_layer(permission_required!(AuthBackend, Role::Admin));

    Router::new()
        .route("/", get(list_classrooms))
        .route("/{id}", get(get_classroom))
        .merge(admin_only_route)
}
