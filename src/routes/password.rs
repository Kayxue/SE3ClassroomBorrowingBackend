use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use chrono::{Duration, Utc};
use nanoid::nanoid;
use redis::{AsyncCommands, RedisError, SetOptions, SetExpiry};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use tracing::warn;
use utoipa::ToSchema;

use crate::{
    AppState, argon_hasher, email_client::send_email, entities::user,
};

const CODE_TTL_SECONDS: u64 = 10 * 60; // 10 minutes
const TOKEN_TTL_SECONDS: u64 = 15 * 60; // 15 minutes

// Redis key prefixes
fn code_key(email: &str) -> String {
    format!("password_reset:code:{}", email)
}

fn token_key(email: &str) -> String {
    format!("password_reset:token:{}", email)
}

#[derive(Serialize, Deserialize)]
struct CodeData {
    code: String,
    expires_at: i64, // Unix timestamp
}

#[derive(Serialize, Deserialize)]
struct TokenData {
    token: String,
    expires_at: i64, // Unix timestamp
}

fn gen_6_digit_code() -> String {
    const DIGITS: [char; 10] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
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
    let email = body.email.trim().to_string();

    // Check if user exists (but always return 200 to avoid email enumeration)
    let exists = match user::Entity::find()
        .filter(user::Column::Email.eq(&email))
        .one(&state.db)
        .await
    {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to query user").into_response();
        }
    };

    if exists {
        let code = gen_6_digit_code();
        let now = Utc::now();
        let expires_at = (now + Duration::minutes(CODE_TTL_SECONDS as i64 / 60)).timestamp();

        let code_data = CodeData {
            code: code.clone(),
            expires_at,
        };

        // Store code in Redis with TTL (this automatically replaces any existing code for this email)
        let mut redis = state.redis.clone();
        let result: Result<(), RedisError> = redis
            .set_options(
                code_key(&email),
                serde_json::to_string(&code_data).unwrap(),
                SetOptions::default().with_expiration(SetExpiry::EX(CODE_TTL_SECONDS)),
            )
            .await;

        if let Err(e) = result {
            warn!(
                "Failed to store password reset code for {} in Redis: {}",
                email, e
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create reset record",
            )
                .into_response();
        }

        // Also delete any existing token for this email (cleanup)
        let _: Result<(), RedisError> = redis.del(token_key(&email)).await;

        let subject = "Password Reset Verification Code";
        let content = format!(
            "Your password reset verification code is: {code}\n\nThis code will expire in {} minutes.",
            CODE_TTL_SECONDS / 60
        );

        if send_email(&email, subject, content).await.is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to send email").into_response();
        }
    }

    (
        StatusCode::OK,
        "If the email exists, a reset code has been sent.",
    )
        .into_response()
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
    let email = body.email.trim().to_owned();
    let code = body.code.trim().to_string();
    let now = Utc::now().timestamp();

    // Get code from Redis
    let mut redis = state.redis.clone();
    let code_str: Option<String> = match redis.get(code_key(&email)).await {
        Ok(c) => c,
        Err(e) => {
            warn!(
                "Failed to get password reset code for {} from Redis: {}",
                email, e
            );
            return (StatusCode::BAD_REQUEST, "Invalid or expired code").into_response();
        }
    };

    let code_data: CodeData = match code_str {
        Some(s) => match serde_json::from_str(&s) {
            Ok(d) => d,
            Err(e) => {
                warn!(
                    "Failed to parse password reset code data for {}: {}",
                    email, e
                );
                return (StatusCode::BAD_REQUEST, "Invalid or expired code").into_response();
            }
        },
        None => return (StatusCode::BAD_REQUEST, "Invalid or expired code").into_response(),
    };

    // Verify code and expiration
    if code_data.code != code || code_data.expires_at <= now {
        return (StatusCode::BAD_REQUEST, "Invalid or expired code").into_response();
    }

    // Generate reset token
    let reset_token = nanoid!(32);
    let expires_at = (Utc::now() + Duration::minutes(TOKEN_TTL_SECONDS as i64 / 60)).timestamp();

    let token_data = TokenData {
        token: reset_token.clone(),
        expires_at,
    };

    // Store token in Redis and delete code (to prevent reuse)
    let result: Result<(), RedisError> = redis
        .set_options(
            token_key(&email),
            serde_json::to_string(&token_data).unwrap(),
            SetOptions::default().with_expiration(SetExpiry::EX(TOKEN_TTL_SECONDS)),
        )
        .await;

    if let Err(e) = result {
        warn!(
            "Failed to store password reset token for {} in Redis: {}",
            email, e
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update reset record",
        )
            .into_response();
    }

    // Delete the code (prevent reuse)
    let _: Result<(), RedisError> = redis.del(code_key(&email)).await;

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
    let email = body.email.trim().to_owned();
    let token = body.reset_token.trim().to_string();

    if body.new_password != body.confirm {
        return (
            StatusCode::BAD_REQUEST,
            "New password and confirm password are not same",
        )
            .into_response();
    }

    let now = Utc::now().timestamp();

    // Get token from Redis
    let mut redis = state.redis.clone();
    let token_str: Option<String> = match redis.get(token_key(&email)).await {
        Ok(t) => t,
        Err(e) => {
            warn!(
                "Failed to get password reset token for {} from Redis: {}",
                email, e
            );
            return (StatusCode::BAD_REQUEST, "Invalid or expired reset token").into_response();
        }
    };

    let token_data: TokenData = match token_str {
        Some(s) => match serde_json::from_str(&s) {
            Ok(d) => d,
            Err(e) => {
                warn!(
                    "Failed to parse password reset token data for {}: {}",
                    email, e
                );
                return (StatusCode::BAD_REQUEST, "Invalid or expired reset token").into_response();
            }
        },
        None => {
            return (StatusCode::BAD_REQUEST, "Invalid or expired reset token").into_response();
        }
    };

    // Verify token and expiration
    if token_data.token != token || token_data.expires_at <= now {
        return (StatusCode::BAD_REQUEST, "Invalid or expired reset token").into_response();
    }

    // Find user in database
    let u = match user::Entity::find()
        .filter(user::Column::Email.eq(&email))
        .one(&state.db)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => return (StatusCode::NOT_FOUND, "User not found").into_response(),
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to query user").into_response();
        }
    };

    // Save user ID before converting to ActiveModel
    let user_id = u.id.clone();

    // Hash new password
    let new_hash = match argon_hasher::hash(body.new_password.as_bytes()).await {
        Ok(h) => h,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password").into_response();
        }
    };

    // Update password in database
    let mut ua: user::ActiveModel = u.into();
    ua.password = Set(new_hash);

    if ua.update(&state.db).await.is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update password",
        )
            .into_response();
    }

    // Invalidate user cache in Redis (password changed)
    let _: Result<(), RedisError> = redis.del(format!("user_{}", user_id)).await;

    // Delete reset token from Redis (successful reset)
    let _: Result<(), RedisError> = redis.del(token_key(&email)).await;

    (StatusCode::OK, "Password reset successfully").into_response()
}

pub fn password_router() -> Router<AppState> {
    Router::new()
        .route("/forgot", post(forgot_password))
        .route("/verify", post(verify_code))
        .route("/reset", post(reset_password))
}
