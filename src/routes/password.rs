use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
};
use chrono::{Duration, Utc};
use nanoid::nanoid;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    ColumnTrait,
    EntityTrait,
    QueryFilter,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    argon_hasher,
    email_client::send_email,
    entities::{password_reset, user},
};

const CODE_TTL_MINUTES: i64 = 10;
const TOKEN_TTL_MINUTES: i64 = 15;

fn gen_6_digit_code() -> String {
    const DIGITS: [char; 10] = ['0','1','2','3','4','5','6','7','8','9'];
    nanoid!(6, &DIGITS)
}

#[derive(Deserialize, ToSchema)]
pub struct ForgotPasswordBody {
    pub email: String,
}

#[derive(Deserialize, ToSchema)]
pub struct VerifyCodeBody {
    pub email: String,
    pub code: String,
}

#[derive(Serialize, ToSchema)]
pub struct VerifyCodeResponse {
    pub reset_token: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ResetPasswordBody {
    pub email: String,
    pub reset_token: String,
    pub new_password: String,
    pub confirm: String,
}

#[utoipa::path(
    post,
    tags = ["Password"],
    description = "Forgot password: input email, send 6-digit code. Always returns 200 to avoid email enumeration.",
    path = "/forgot",
    request_body(content = ForgotPasswordBody, content_type = "application/json"),
    responses(
        (status = 200, description = "If email exists, code has been sent", body = String),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn forgot_password(
    State(state): State<AppState>,
    Json(body): Json<ForgotPasswordBody>,
) -> impl IntoResponse {
    let email = body.email.trim().to_lowercase();

    // 先查 user 是否存在（但回應一律 200）
    let exists = match user::Entity::find()
        .filter(user::Column::Email.eq(email.clone()))
        .one(&state.db)
        .await
    {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to query user").into_response(),
    };

    if exists {
        let code = gen_6_digit_code();
        let now = Utc::now();
        let code_exp = now + Duration::minutes(CODE_TTL_MINUTES);

        // 同 email 只保留一筆（先刪再建）
        let _ = password_reset::Entity::delete_many()
            .filter(password_reset::Column::Email.eq(email.clone()))
            .exec(&state.db)
            .await;

        let record = password_reset::ActiveModel {
            id: Set(nanoid!()),
            email: Set(email.clone()),
            code: Set(Some(code.clone())),
            code_expires_at: Set(Some(code_exp.into())),
            reset_token: Set(None),
            reset_token_expires_at: Set(None),
            created_at: NotSet,
            updated_at: NotSet,
        };

        if record.insert(&state.db).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create reset record").into_response();
        }

        let subject = "Password Reset Verification Code";
        let content = format!(
            "Your password reset verification code is: {code}\n\nThis code will expire in {CODE_TTL_MINUTES} minutes."
        );

        if send_email(&email, subject, content).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to send email").into_response();
        }
    }

    (StatusCode::OK, "If the email exists, a reset code has been sent.").into_response()
}

#[utoipa::path(
    post,
    tags = ["Password"],
    description = "Verify code: returns reset_token for final reset step.",
    path = "/verify",
    request_body(content = VerifyCodeBody, content_type = "application/json"),
    responses(
        (status = 200, description = "Code verified", body = VerifyCodeResponse),
        (status = 400, description = "Invalid or expired code", body = String),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn verify_code(
    State(state): State<AppState>,
    Json(body): Json<VerifyCodeBody>,
) -> impl IntoResponse {
    let email = body.email.trim().to_lowercase();
    let code = body.code.trim().to_string();
    let now = Utc::now();
    let now_tz: sea_orm::prelude::DateTimeWithTimeZone = now.into();

    let rec = match password_reset::Entity::find()
        .filter(password_reset::Column::Email.eq(email.clone()))
        .one(&state.db)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::BAD_REQUEST, "Invalid or expired code").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to query reset record").into_response(),
    };

    let code_ok = match (rec.code.clone(), rec.code_expires_at) {
        (Some(saved), Some(exp)) => saved == code && exp > now_tz,
        _ => false,
    };

    if !code_ok {
        return (StatusCode::BAD_REQUEST, "Invalid or expired code").into_response();
    }

    let reset_token = nanoid!(32);
    let token_exp = now + Duration::minutes(TOKEN_TTL_MINUTES);

    // 更新 record：發 token、清掉 code（避免重複驗證）
    let mut active: password_reset::ActiveModel = rec.into();
    active.reset_token = Set(Some(reset_token.clone()));
    active.reset_token_expires_at = Set(Some(token_exp.into()));
    active.code = Set(None);
    active.code_expires_at = Set(None);

    if active.update(&state.db).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to update reset record").into_response();
    }

    (StatusCode::OK, Json(VerifyCodeResponse { reset_token })).into_response()
}

#[utoipa::path(
    post,
    tags = ["Password"],
    description = "Reset password using reset_token.",
    path = "/reset",
    request_body(content = ResetPasswordBody, content_type = "application/json"),
    responses(
        (status = 200, description = "Password reset successfully", body = String),
        (status = 400, description = "Bad request", body = String),
        (status = 404, description = "User not found", body = String),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn reset_password(
    State(state): State<AppState>,
    Json(body): Json<ResetPasswordBody>,
) -> impl IntoResponse {
    let email = body.email.trim().to_lowercase();
    let token = body.reset_token.trim().to_string();

    if body.new_password != body.confirm {
        return (StatusCode::BAD_REQUEST, "New password and confirm password are not same").into_response();
    }

    let now = Utc::now();
    let now_tz: sea_orm::prelude::DateTimeWithTimeZone = now.into();

    let rec = match password_reset::Entity::find()
        .filter(password_reset::Column::Email.eq(email.clone()))
        .one(&state.db)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::BAD_REQUEST, "Invalid or expired reset token").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to query reset record").into_response(),
    };

    let token_ok = match (rec.reset_token.clone(), rec.reset_token_expires_at) {
        (Some(saved), Some(exp)) => saved == token && exp > now_tz,
        _ => false,
    };

    if !token_ok {
        return (StatusCode::BAD_REQUEST, "Invalid or expired reset token").into_response();
    }

    // 找 user
    let u = match user::Entity::find()
        .filter(user::Column::Email.eq(email.clone()))
        .one(&state.db)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => return (StatusCode::NOT_FOUND, "User not found").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to query user").into_response(),
    };

    // 更新密碼
    let new_hash = match argon_hasher::hash(body.new_password.as_bytes()).await {
        Ok(h) => h,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password").into_response(),
    };

    let mut ua: user::ActiveModel = u.into();
    ua.password = Set(new_hash);

    if ua.update(&state.db).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to update password").into_response();
    }

    // 重置成功：刪掉 reset record
    let _ = password_reset::Entity::delete_many()
        .filter(password_reset::Column::Email.eq(email))
        .exec(&state.db)
        .await;

    (StatusCode::OK, "Password reset successfully").into_response()
}

pub fn password_router() -> Router<AppState> {
    Router::new()
        .route("/forgot", post(forgot_password))
        .route("/verify", post(verify_code))
        .route("/reset", post(reset_password))
}
