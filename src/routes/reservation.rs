use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{post, put},
};
use axum_login::{permission_required, login_required};
use sea_orm::{
    ActiveModelTrait, EntityTrait,
    ActiveValue::Set
};
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
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create reservation").into_response(),
    }
}



#[derive(Deserialize, ToSchema)]
pub struct ReviewReservationBody {
    status: ReservationStatus,
    reject_reason: Option<String>,
}

#[utoipa::path(
    put,
    tags = ["Reservation"],
    description = "Review a reservation",
    path = "/{id}",
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
        Ok(Some(reservation)) => {
            let mut reservation: reservation::ActiveModel = reservation.into();
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



pub fn reservation_router() -> Router<AppState> {
    let admin_only_route = Router::new()
        .route("/{id}", put(review_reservation))
        .route_layer(permission_required!(AuthBackend, Role::Admin));

    Router::new()
        // .route("/", get(list_reservations).post(create_reservation))
        .route("/", post(create_reservation)) 
        .merge(admin_only_route)
}
