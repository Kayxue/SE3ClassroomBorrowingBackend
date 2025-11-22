use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use sea_orm::{EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{AppState, entities::{reservation, classroom}};

#[derive(Serialize, ToSchema)]
pub struct ReservationHistoryResponse {
    classroom_name: String,
    reservation_id: String,
    purpose: String,
    start_time: String,
    end_time: String,
}

#[utoipa::path(
    get,
    tags = ["Reservation"],
    description = "Get reservation history of a classroom",
    path = "/classroom/{id}/history",
    responses(
        (status = 200, description = "List of reservations for the classroom", body = Vec<ReservationHistoryResponse>),
        (status = 404, description = "Classroom not found"),
        (status = 500, description = "Internal server error"),
    ),
    params(
        ("id" = String, Path, description = "Classroom ID")
    ),
    security(
        ("session_cookie" = [])
    )
)]
pub async fn reservation_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match classroom::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(classroom)) => {
            let reservations = reservation::Entity::find()
                .filter(reservation::Column::ClassroomId.eq(classroom.id))
                .all(&state.db)
                .await;

            match reservations {
                Ok(reservations) => {
                    let response: Vec<ReservationHistoryResponse> = reservations
                        .into_iter()
                        .map(|res| ReservationHistoryResponse {
                            classroom_name: classroom.name,
                            reservation_id: res.id,
                            purpose: res.purpose,
                            start_time: res.start_time.to_string(),
                            end_time: res.end_time.to_string(),
                        })
                        .collect();

                    (StatusCode::OK, Json(response)).into_response()
                }
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to fetch reservation history",
                )
                    .into_response(),
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

pub fn reservation_history_router() -> Router<AppState> {
    Router::new().route("/classroom/:id/history", get(reservation_history))
}
