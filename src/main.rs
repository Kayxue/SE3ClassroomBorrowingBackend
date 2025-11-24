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

mod argonhasher;
mod entities;
mod loginsystem;
mod routes;

use argonhasher::hash;
use loginsystem::AuthBackend;
use routes::classroom::classroom_router;
use routes::reservation::reservation_router;
use routes::user::user_router;
use routes::key::key_router;

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
        (name = "Key", description = "Key endpoints")
    ),
    paths(
        routes::key::create_key,
        routes::key::update_key,
        routes::key::delete_key,
    ),
    components(schemas(
        entities::key::Model,
        entities::key::Relation,
        routes::key::CreateKeyBody,
        routes::key::UpdateKeyBody,
        routes::key::KeyResponse,
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
    ),
    components(schemas(
        entities::reservation::Model,
        entities::sea_orm_active_enums::ReservationStatus,
        routes::reservation::ReviewReservationBody,
        routes::reservation::CreateReservationBody,
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
        loginsystem::Credentials,
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
    nest((path = "/user", api = UserApi), (path = "/classroom", api = ClassroomApi), (path = "/reservation", api = ReservationApi), (path = "/key", api = KeyApi)),
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
            loginsystem::Credentials,
            routes::user::RegisterBody,
            routes::classroom::CreateClassroomBody,
            entities::classroom::Model,
            entities::sea_orm_active_enums::ClassroomStatus,
            routes::user::UserResponse,
            routes::user::UpdatePasswordBody,
            routes::reservation::ReviewReservationBody,
            entities::reservation::Model,
            entities::sea_orm_active_enums::ReservationStatus,
            routes::classroom::GetClassroomResponse,
            routes::classroom::GetClassroomKeyResponse,
            routes::classroom::GetClassroomReservationResponse,
            routes::classroom::GetClassroomKeyReservationResponse,
            entities::key::Model,
            entities::reservation::Model,
            routes::classroom::UpdateClassroomBody,
            routes::classroom::UpdateClassroomPhotoBody,
            routes::key::CreateKeyBody
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

    let argon2_config = argonhasher::Config {
        iterations: 4,
        parallelism: 4,
        memory_cost: 512,
        secret_key: password_hashing_secret.into_bytes(),
    };

    argonhasher::set_config(argon2_config);

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
        .with_state(app_state)
        .merge(Scalar::with_url("/docs", ApiDoc::openapi()))
        .layer(ServiceBuilder::new().layer(auth_layer));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
