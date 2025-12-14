use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use axum_login::{login_required, permission_required};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, QueryOrder};
use serde::Deserialize;
use string_builder::Builder;
use utoipa::ToSchema;
use sea_orm::prelude::DateTime;
use chrono::{NaiveDateTime, DateTime as ChronoDateTime};

use crate::{
    AppState,
    email_client::send_email,
    entities::{
        reservation,
        sea_orm_active_enums::{ReservationStatus, Role},
        user,
    },
    login_system::{AuthBackend, AuthSession},
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

#[derive(Deserialize, ToSchema)]
pub struct GetReservationsQuery {
    pub status: Option<ReservationStatus>,
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
        Ok(model) => {
            let _ = send_email(
                user.email,
                "Reservation Created",
                format!(
                    "Your reservation has been created. Reservation ID: {}",
                    model.id
                ),
            )
            .await
            .unwrap();
            match user::Entity::find()
                .filter(user::Column::Role.eq(Role::Admin))
                .all(&state.db)
                .await
            {
                Ok(admins) => {
                    for admin in admins {
                        let _ = send_email(
                            admin.email,
                            format!("New Reservation Request: {}", model.id),
                            format!(
                                "There is a new reservation request. Reservation ID: {}",
                                model.id
                            ),
                        );
                    }
                }
                Err(_) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch admins")
                        .into_response();
                }
            }
            (StatusCode::CREATED, Json(model)).into_response()
        }
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
                Ok(reservation_updated) => {
                    let user = match user::Entity::find_by_id(
                        reservation_updated.user_id.as_ref().unwrap(),
                    )
                    .one(&state.db)
                    .await
                    {
                        Ok(Some(u)) => u,
                        Ok(None) => {
                            return (StatusCode::NOT_FOUND, "User not found").into_response();
                        }
                        Err(_) => {
                            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch user")
                                .into_response();
                        }
                    };

                    let mut body_builder = Builder::default();
                    body_builder.append("Your reservation has been reviewed.\nStatus: ");
                    body_builder.append(format!("{:?}", reservation_updated.status));
                    if reservation_updated.status == ReservationStatus::Rejected {
                        if let Some(ref reason) = reservation_updated.reject_reason {
                            body_builder.append("\nReason: ");
                            body_builder.append(reason.as_str());
                        }
                    }
                    let email_body = body_builder.string().unwrap();

                    send_email(
                        user.email,
                        format!(
                            "Reservation has been reviewed: {:?}",
                            reservation_updated.id
                        ),
                        email_body,
                    )
                    .await
                    .unwrap();
                    (StatusCode::OK, "Reservation reviewed successfully").into_response()
                }
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
//   Get Reservations by Status
// ===============================
#[utoipa::path(
    get,
    tags = ["Reservation"],
    description = "Get all reservations (Admin only)",
    path = "",
    responses(
        (status = 200, description = "List of reservations with the specified status", body = [reservation::Model]),
        (status = 500, description = "Failed to fetch reservations")
    ),
    params(
        ("status" = Option<ReservationStatus>, Query, description = "Status of the reservations to fetch")
    ),
    security(("session_cookie" = []))
)]
pub async fn get_reservations(
    State(state): State<AppState>,
    Query(query): Query<GetReservationsQuery>,
) -> impl IntoResponse {
    let mut find_query = reservation::Entity::find();

    if let Some(status) = query.status {
        find_query = find_query.filter(reservation::Column::Status.eq(status));
    }

    match find_query.all(&state.db).await {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch reservations",
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    tags = ["Reservation"],
    description = "Get all reservations for self",
    path = "/self",
    responses(
        (status = 200, description = "List of all reservations", body = [reservation::Model]),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_all_reservations_for_self(
    session: AuthSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user = session.user.unwrap();
    let reservations = match reservation::Entity::find()
        .filter(reservation::Column::UserId.eq(user.id))
        .all(&state.db)
        .await
    {
        Ok(reservations) => reservations,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch reservations",
            )
                .into_response();
        }
    };
    (StatusCode::OK, Json(reservations)).into_response()
}

#[utoipa::path(
    delete,
    tags = ["Reservation"],
    description = "Cancel a reservation",
    path = "/{id}",
    responses(
        (status = 200, description = "Reservation cancelled successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Reservation not found"),
        (status = 500, description = "Failed to cancel reservation"),
    ),
    params(("id" = String, Path)),
    security(("session_cookie" = []))
)]
pub async fn cancel_reservation(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match session.user {
        Some(u) => u,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let reservation = match reservation::Entity::find_by_id(&id).one(&state.db).await {
        Ok(Some(reservation)) => reservation,
        Ok(None) => return (StatusCode::NOT_FOUND, "Reservation not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch reservation",
            )
                .into_response();
        }
    };

    if reservation.user_id != Some(user.id) {
        return (
            StatusCode::FORBIDDEN,
            "You can only cancel your own reservation",
        )
            .into_response();
    }

    if reservation.status != ReservationStatus::Pending {
        return (
            StatusCode::BAD_REQUEST,
            "Only pending reservations can be cancelled",
        )
            .into_response();
    }

    match reservation.delete(&state.db).await {
        Ok(_) => (StatusCode::OK, "Reservation cancelled successfully").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to cancel reservation",
        )
            .into_response(),
    }
}
// ===============================
//   SelfListQuery
// ===============================
#[derive(Deserialize, ToSchema)]
pub struct SelfListQuery {
    pub status: Option<ReservationStatus>,
    pub classroom_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub sort: Option<String>, // asc | desc
}

fn parse_dt(s: &str) -> Result<DateTime, ()> {
    if let Ok(dt) = ChronoDateTime::parse_from_rfc3339(s) {
        return Ok(dt.naive_utc());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(dt);
    }
    Err(())
}

#[utoipa::path(
    get,
    tags = ["Reservation"],
    description = "Get reservations for self with filters (time range, classroom, status) and sorting",
    path = "/self/list",
    params(
        ("status" = Option<ReservationStatus>, Query, description = "Filter by status"),
        ("classroom_id" = Option<String>, Query, description = "Filter by classroom id"),
        ("from" = Option<String>, Query, description = "Filter: start_time >= from (ISO8601)"),
        ("to" = Option<String>, Query, description = "Filter: start_time <= to (ISO8601)"),
        ("sort" = Option<String>, Query, description = "Sort by start_time: asc|desc (default desc)")
    ),
    responses(
        (status = 200, description = "List of reservations", body = [reservation::Model]),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid query"),
        (status = 500, description = "Failed to fetch reservations")
    ),
    security(("session_cookie" = []))
)]
pub async fn get_self_reservations_filtered(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<SelfListQuery>,
) -> impl IntoResponse {
    let user = match session.user {
        Some(u) => u,
        None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    };

    let mut find_query = reservation::Entity::find()
        .filter(reservation::Column::UserId.eq(Some(user.id)));

    if let Some(status) = query.status {
        find_query = find_query.filter(reservation::Column::Status.eq(status));
    }

    if let Some(classroom_id) = query.classroom_id {
        find_query = find_query.filter(reservation::Column::ClassroomId.eq(Some(classroom_id)));
    }

    if let Some(from) = query.from {
        let from_dt: DateTime = match parse_dt(&from) {
            Ok(v) => v,
            Err(_) => return (StatusCode::BAD_REQUEST, "Invalid 'from'").into_response(),
        };
        find_query = find_query.filter(reservation::Column::StartTime.gte(from_dt));
    }

    if let Some(to) = query.to {
        let to_dt: DateTime = match parse_dt(&to) {
            Ok(v) => v,
            Err(_) => return (StatusCode::BAD_REQUEST, "Invalid 'to'").into_response(),
        };
        find_query = find_query.filter(reservation::Column::StartTime.lte(to_dt));
    }

    match query.sort.as_deref() {
        Some("asc") => find_query = find_query.order_by_asc(reservation::Column::StartTime),
        Some("desc") | None => find_query = find_query.order_by_desc(reservation::Column::StartTime),
        Some(_) => return (StatusCode::BAD_REQUEST, "Invalid 'sort'").into_response(),
    }

    match find_query.all(&state.db).await {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch reservations").into_response(),
    }
}


// ===============================
//   Reservation Router
// ===============================
pub fn reservation_router() -> Router<AppState> {
    let admin_only_route = Router::new()
        .route("/{id}/review", put(review_reservation))
        .route("/", get(get_reservations))
        .route_layer(permission_required!(AuthBackend, Role::Admin));

    let login_required_route = Router::new()
        .route("/", post(create_reservation))
        .route("/self", get(get_all_reservations_for_self))
        .route("/self/list", get(get_self_reservations_filtered))
        .route("/{id}", put(update_reservation))
        .route("/{id}", delete(cancel_reservation))
        .route_layer(login_required!(AuthBackend));

    Router::new()
        .merge(admin_only_route)
        .merge(login_required_route)
}
