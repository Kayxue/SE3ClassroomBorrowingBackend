use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::put,
};
use axum_login::permission_required;
use sea_orm::{ActiveModelTrait, EntityTrait};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    AppState,
    entities::{
        reservation,
        sea_orm_active_enums::{ReservationStatus, Role},
    },
    loginsystem::AuthBackend,
};

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
        (status = 200, description = "Reservation reviewed successfully", body = String),
        (status = 404, description = "Reservation not found", body = String),
        (status = 500, description = "Failed to review reservation", body = String),
    ),
    params(
        ("id" = String, Path, description = "The ID of the reservation to review"),
    ),
    security(
        ("session_cookie" = [])
    )
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
            reservation.status = sea_orm::Set(status);
            reservation.reject_reason = sea_orm::Set(reject_reason);
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
        .merge(admin_only_route)
}
