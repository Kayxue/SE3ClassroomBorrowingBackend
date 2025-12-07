use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
};
use axum_login::permission_required;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, ColumnTrait, QueryFilter};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    AppState,
    entities::{
        reservation,
        sea_orm_active_enums::{ReservationStatus, Role},
    },
    loginsystem::{AuthBackend, AuthSession},
};

use nanoid::nanoid;

// ===============================
//   Create Reservation (User)
// ===============================
#[derive(Deserialize, ToSchema)]
pub struct CreateReservationBody {
    pub classroom_id: String,
    pub purpose: String,
    pub start_time: String,
    pub end_time: String,
}

#[utoipa::path(
    post,
    tags = ["Reservation"],
    description = "Submit a classroom reservation request",
    path = "",
    request_body(content = CreateReservationBody, content_type = "application/json"),
    responses(
        (status = 201, description = "Reservation created", body = reservation::Model),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Failed to create reservation")
    ),
    security(("session_cookie" = []))
)]
pub async fn create_reservation(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<CreateReservationBody>,
) -> impl IntoResponse {
    let user = match session.user {
        Some(u) => u,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let new_reservation = reservation::ActiveModel {
        id: Set(nanoid!()),
        user_id: Set(Some(user.id)),
        classroom_id: Set(Some(body.classroom_id)),
        purpose: Set(body.purpose),
        start_time: Set(body.start_time.parse().unwrap()),
        end_time: Set(body.end_time.parse().unwrap()),
        approved_by: Set(None),
        reject_reason: Set(None),
        cancel_reason: Set(None),
        status: Set(ReservationStatus::Pending),
    };

    match new_reservation.insert(&state.db).await {
        Ok(model) => (StatusCode::CREATED, Json(model)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create reservation",
        )
            .into_response(),
    }
}

// ===============================
//   Review Reservation (Admin)
// ===============================
#[derive(Deserialize, ToSchema)]
pub struct ReviewReservationBody {
    pub status: ReservationStatus,
    pub reject_reason: Option<String>,
}

#[utoipa::path(
    put,
    tags = ["Reservation"],
    description = "Review a reservation (Admin only)",
    path = "/{id}/review",
    request_body(content = ReviewReservationBody, content_type = "application/json"),
    responses(
        (status = 200, body = String),
        (status = 404, body = String),
        (status = 500, body = String),
    ),
    params(("id" = String, Path)),
    security(("session_cookie" = []))
)]
pub async fn review_reservation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ReviewReservationBody>,
) -> impl IntoResponse {
    let ReviewReservationBody {
        status,
        reject_reason,
    } = body;

    match reservation::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(res_model)) => {
            let mut reservation: reservation::ActiveModel = res_model.into();
            reservation.status = Set(status);
            reservation.reject_reason = Set(reject_reason);

            match reservation.update(&state.db).await {
                Ok(_) => (StatusCode::OK, "Reservation reviewed successfully").into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to review reservation",
                )
                    .into_response(),
            }
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Reservation not found").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to review reservation",
        )
            .into_response(),
    }
}

// ===============================
//   Update Reservation (User)
// ===============================
#[derive(Deserialize, ToSchema)]
pub struct UpdateReservationBody {
    pub purpose: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

#[utoipa::path(
    put,
    tags = ["Reservation"],
    description = "Update own reservation request (only when pending)",
    path = "/{id}",
    request_body(content = UpdateReservationBody, content_type = "application/json"),
    responses(
        (status = 200, description = "Reservation updated", body = reservation::Model),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Reservation not found"),
        (status = 400, description = "Only pending reservations can be updated"),
        (status = 500, description = "Failed to update reservation")
    ),
    params(("id" = String, Path)),
    security(("session_cookie" = []))
)]
pub async fn update_reservation(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateReservationBody>,
) -> impl IntoResponse {
    let user = match session.user {
        Some(u) => u,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let UpdateReservationBody {
        purpose,
        start_time,
        end_time,
    } = body;

    let res_model = match reservation::Entity::find_by_id(&id).one(&state.db).await {
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

    if res_model.user_id != Some(user.id.clone()) {
        return (
            StatusCode::FORBIDDEN,
            "You can only update your own reservation",
        )
            .into_response();
    }

    if res_model.status != ReservationStatus::Pending {
        return (
            StatusCode::BAD_REQUEST,
            "Only pending reservations can be updated",
        )
            .into_response();
    }

    let mut reservation: reservation::ActiveModel = res_model.into();

    if let Some(p) = purpose {
        reservation.purpose = Set(p);
    }
    if let Some(start) = start_time {
        reservation.start_time = Set(start.parse().unwrap());
    }
    if let Some(end) = end_time {
        reservation.end_time = Set(end.parse().unwrap());
    }

    match reservation.update(&state.db).await {
        Ok(updated) => (StatusCode::OK, Json(updated)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update reservation",
        )
            .into_response(),
    }
}
// ===============================
//   Get Pending Reservations
// ===============================
#[utoipa::path(
    get,
    tags = ["Reservation"],
    description = "Get all pending reservation requests (Admin only)",
    path = "/pending",
    responses(
        (status = 200, description = "List of pending reservations", body = [reservation::Model]),
        (status = 500, description = "Failed to fetch pending reservations")
    ),
    security(("session_cookie" = []))
)]
pub async fn get_pending_reservations(
    State(state): State<AppState>,
) -> impl IntoResponse {
    match reservation::Entity::find()
        .filter(reservation::Column::Status.eq(ReservationStatus::Pending))
        .all(&state.db)
        .await
    {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch pending reservations",
        )
            .into_response(),
    }
}
// ===============================
//   Reservation Router
// ===============================
pub fn reservation_router() -> Router<AppState> {
    let admin_only_route = Router::new()
        .route("/{id}/review", put(review_reservation))
        .route_layer(permission_required!(AuthBackend, Role::Admin));

    let user_reservation_route = Router::new()
        .route("/", post(create_reservation))
        .route("/{id}", put(update_reservation));
        .route("/pending", get(get_pending_reservations))

    Router::new()
        .merge(user_reservation_route)
        .merge(admin_only_route)
}
