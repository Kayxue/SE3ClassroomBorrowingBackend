use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use sea_orm::{EntityTrait, QueryFilter, ColumnTrait};
use serde::Serialize;

use crate::{
    AppState,
    entities::reservation,
};

#[derive(Serialize)]
pub struct ClassroomReservationList {
    pub classroom_id: String,
    pub reservations: Vec<reservation::Model>,
}

pub async fn list_reservations_by_classroom(
    State(state): State<AppState>,
    Path(classroom_id): Path<String>,
) -> impl IntoResponse {
    let db = &state.db;

    let res = reservation::Entity::find()
        .filter(reservation::Column::ClassroomId.eq(classroom_id.clone()))
        .all(db)
        .await;

    match res {
        Ok(list) => {
            let body = ClassroomReservationList {
                classroom_id,
                reservations: list,
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch reservation history",
        )
            .into_response(),
    }
}
