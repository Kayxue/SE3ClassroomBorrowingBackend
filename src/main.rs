#[cfg(all(target_env = "musl", not(target_os = "macos")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::net::SocketAddr;

use axum::{Router, extract::Path, response::IntoResponse, routing::get};
use axum_login::AuthManagerLayerBuilder;
use dotenv::dotenv;
use nanoid::nanoid;
use sea_orm::{Database, DatabaseConnection};
use std::env;
use tower::ServiceBuilder;
use tower_sessions::{
    Expiry, SessionManagerLayer,
    cookie::{SameSite, time::Duration},
};
use tower_sessions_redis_store::{
    RedisStore,
    fred::prelude::{ClientLike, Config, Pool, Server, ServerConfig},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa_scalar::{Scalar, Servable};

mod argon_hasher;
mod email_client;
mod entities;
mod login_system;
mod routes;
mod utils;
#[cfg(test)]
mod utils_test;

use argon_hasher::hash;
use login_system::AuthBackend;
use routes::announcement::announcement_router;
use routes::classroom::classroom_router;
use routes::infraction::infraction_router;
use routes::key::key_router;
use routes::reservation::reservation_router;
use routes::user::user_router;
use routes::black_list::black_list_router;

use crate::email_client::{EmailClientConfig, set_email_client_config};

#[utoipa::path(
    get,
    description = "Returns the Argon2 hash of the provided password",
    tags = ["Root"],
    path = "/argon2/{password}",
    responses(
        (status = 200, description = "Returns the Argon2 hash of the provided password", body = String),
    ),
    params(
        ("password" = String, Path, description = "The password to be hashed"),
    )
)]
async fn argon2(Path(password): Path<String>) -> impl IntoResponse {
    let hash = hash(password.as_bytes()).await.unwrap();
    hash
}

#[utoipa::path(
    get,
    description = "Returns a newly generated NanoID",
    tags = ["Root"],
    path = "/nanoid",
    responses(
        (status = 200, description = "Returns a newly generated NanoID", body = String),
    ),
)]
async fn nanoid() -> impl IntoResponse {
    nanoid!()
}

#[utoipa::path(
    get,
    description = "Returns a greeting message",
    tags = ["Root"],
    path = "/",
    responses(
        (status = 200, description = "Returns a greeting message", body = String),
    ),
)]
async fn root() -> impl IntoResponse {
    "Hello, World!"
}

#[derive(Clone)]
struct AppState {
    db: DatabaseConnection,
}

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "session_cookie",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("id"))),
            )
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    tags(
        (name = "Infraction", description = "Infraction endpoints")
    ),
    paths(
        routes::infraction::create_infraction,
        routes::infraction::update_infraction,
        routes::infraction::delete_infraction,
        routes::infraction::list_infractions,
        routes::infraction::get_infraction,
    ),
    components(schemas(
        entities::infraction::Model,
        routes::infraction::CreateInfractionBody,
        routes::infraction::UpdateInfractionBody,
    ))
)]
struct InfractionApi;

#[derive(OpenApi)]
#[openapi(
    tags(
        (name = "Announcement", description = "Announcement endpoints")
    ),
    paths(
        routes::announcement::create_announcement,
        routes::announcement::list_announcements,
        routes::announcement::get_announcement,
        routes::announcement::delete_announcement,
    ),
    components(schemas(
        entities::announcement::Model,
        routes::announcement::CreateAnnouncementBody,
    ))
)]
struct AnnouncementApi;

#[derive(OpenApi)]
#[openapi(
    tags(
        (name = "Key", description = "Key endpoints")
    ),
    paths(
        routes::key::create_key,
        routes::key::update_key,
        routes::key::delete_key,
        routes::key::borrow_key,
        routes::key::return_key
    ),
    components(schemas(
        entities::key::Model,
        entities::classroom::Model,
        routes::key::CreateKeyBody,
        routes::key::UpdateKeyBody,
        routes::key::KeyResponse,
        routes::key::BorrowKeyBody,
        routes::key::ReturnKeyBody
    ))
)]
struct KeyApi;

#[derive(OpenApi)]
#[openapi(
    tags(
        (name = "Reservation", description = "Reservation endpoints")
    ),
    paths(
        routes::reservation::review_reservation,
        routes::reservation::create_reservation,
        routes::reservation::update_reservation,
        routes::reservation::get_reservations,
        routes::reservation::get_all_reservations_for_self,
        routes::reservation::admin_list_reservations,
        routes::reservation::cancel_reservation,
        routes::reservation::get_self_reservations_filtered
    ),
    components(schemas(
        entities::reservation::Model,
        entities::sea_orm_active_enums::ReservationStatus,
        routes::reservation::ReviewReservationBody,
        routes::reservation::CreateReservationBody,
        routes::reservation::UpdateReservationBody,
        routes::reservation::GetReservationsQuery,
        routes::reservation::SelfListQuery,
        routes::reservation::AdminListQuery,
        routes::reservation::PagedReservations
    ))
)]
struct ReservationApi;

#[derive(OpenApi)]
#[openapi(
    tags(
        (name = "User", description = "User endpoints")
    ),
    paths(
        routes::user::register,
        routes::user::login,
        routes::user::logout,
        routes::user::profile,
        routes::user::get_user,
        routes::user::update_password,
        routes::user::update_profile
    ),
    components(schemas(
        entities::user::Model,
        entities::sea_orm_active_enums::Role,
        login_system::Credentials,
        routes::user::RegisterBody,
        routes::user::UpdatePasswordBody,
        routes::user::UserResponse,
        routes::user::UpdateProfileBody
    ))
)]
struct UserApi;

#[derive(OpenApi)]
#[openapi(
    tags(
        (name = "Classroom", description = "Classroom endpoints")
    ),
    paths(
        routes::classroom::create_classroom,
        routes::classroom::get_classroom,
        routes::classroom::list_classrooms,
        routes::classroom::update_classroom,
        routes::classroom::update_classroom_photo,
        routes::classroom::delete_classroom
    ),
    components(schemas(
        routes::classroom::CreateClassroomBody,
        entities::classroom::Model,
        entities::sea_orm_active_enums::ClassroomStatus,
        routes::classroom::GetClassroomResponse,
        routes::classroom::GetClassroomKeyResponse,
        routes::classroom::GetClassroomReservationResponse,
        routes::classroom::GetClassroomKeyReservationResponse,
        routes::classroom::UpdateClassroomBody,
        routes::classroom::UpdateClassroomPhotoBody,
        entities::key::Model,
        entities::reservation::Model,
    ))
)]
struct ClassroomApi;

#[derive(OpenApi)]
#[openapi(
    nest((path = "/user", api = UserApi), (path = "/classroom", api = ClassroomApi), (path = "/reservation", api = ReservationApi), (path = "/key", api = KeyApi), (path = "/announcement", api = AnnouncementApi), (path = "/infraction", api = InfractionApi)),
    tags((name = "Root", description = "Root endpoints")),
    paths(
        root,
        nanoid,
        argon2,
    ),
    modifiers(&SecurityAddon),
    info(title = "Classroom Borrowing API", version = "1.0"),
    servers(
        (url = "/api", description = "Base API path when hosting"),
        (url = "/", description = "Base API path when running on local")
    ),
    components(
        schemas(
            entities::user::Model,
            entities::sea_orm_active_enums::Role,
            login_system::Credentials,
            routes::user::RegisterBody,
            routes::classroom::CreateClassroomBody,
            entities::classroom::Model,
            entities::sea_orm_active_enums::ClassroomStatus,
            routes::user::UserResponse,
            routes::user::UpdatePasswordBody,
            routes::reservation::ReviewReservationBody,
            entities::reservation::Model,
            routes::reservation::GetReservationsQuery,
            entities::sea_orm_active_enums::ReservationStatus,
            routes::classroom::GetClassroomResponse,
            routes::classroom::GetClassroomKeyResponse,
            routes::classroom::GetClassroomReservationResponse,
            routes::classroom::GetClassroomKeyReservationResponse,
            entities::key::Model,
            entities::reservation::Model,
            routes::classroom::UpdateClassroomBody,
            routes::classroom::UpdateClassroomPhotoBody,
            routes::key::CreateKeyBody,
            routes::announcement::CreateAnnouncementBody,
            entities::announcement::Model,
            routes::key::BorrowKeyBody,
            routes::key::ReturnKeyBody,
            routes::infraction::CreateInfractionBody,
            routes::infraction::UpdateInfractionBody,
            entities::infraction::Model,
        )
    )
)]
struct ApiDoc;

#[tokio::main]
async fn main() {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=debug", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let password_hashing_secret =
        env::var("PASSWORD_HASHING_SECRET").expect("PASSWORD_HASHING_SECRET must be set");

    let argon2_config = argon_hasher::Argon2Config {
        iterations: 4,
        parallelism: 4,
        memory_cost: 512,
        secret_key: password_hashing_secret.into_bytes(),
    };

    argon_hasher::set_config(argon2_config);

    let email_client_config = EmailClientConfig {
        smtp_server: env::var("SMTP_SERVER").expect("SMTP_SERVER must be set"),
        smtp_port: env::var("SMTP_PORT")
            .expect("SMTP_PORT must be set")
            .parse()
            .unwrap(),
        username: env::var("SMTP_USERNAME").expect("SMTP_USERNAME must be set"),
        password: env::var("SMTP_PASSWORD").expect("SMTP_PASSWORD must be set"),
    };

    set_email_client_config(email_client_config);

    let redis_pool_config = Config {
        server: ServerConfig::Centralized {
            server: Server {
                host: env::var("REDIS_IP")
                    .unwrap_or_else(|_| "localhost".into())
                    .parse()
                    .unwrap(),
                port: env::var("REDIS_PORT")
                    .unwrap_or_else(|_| "6379".into())
                    .parse()
                    .unwrap(),
            },
        },
        ..Default::default()
    };
    let pool = Pool::new(redis_pool_config, None, None, None, 6).unwrap();
    let _ = pool.connect();
    pool.wait_for_connect().await.unwrap();
    let session_store = RedisStore::new(pool);
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(Duration::days(1)))
        .with_same_site(SameSite::Lax);

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = Database::connect(&database_url).await.unwrap();

    let auth_backend = AuthBackend::new(db.clone());
    let auth_layer = AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

    let image_service_ip = env::var("IMAGE_SERVICE_IP").expect("IMAGE_SERVICE_IP must be set");
    let image_service_api_key =
        env::var("IMAGE_SERVICE_API_KEY").expect("IMAGE_SERVICE_API_KEY must be set");

    let app_state = AppState { db: db };

    let app = Router::new()
        .route("/", get(root))
        .route("/nanoid", get(nanoid))
        .route("/argon2/{password}", get(argon2))
        .nest("/user", user_router())
        .nest(
            "/classroom",
            classroom_router(image_service_ip, image_service_api_key),
        )
        .nest("/reservation", reservation_router())
        .nest("/key", key_router())
        .nest("/announcement", announcement_router())
        .nest("/infraction", infraction_router())
        .nest("/black_list", black_list_router())
        .with_state(app_state)
        .merge(Scalar::with_url("/docs", ApiDoc::openapi()))
        .layer(ServiceBuilder::new().layer(auth_layer));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
