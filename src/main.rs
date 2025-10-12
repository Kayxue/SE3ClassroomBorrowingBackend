use std::net::SocketAddr;

use axum::{Router, extract::Path, response::IntoResponse, routing::get};
use dotenv::dotenv;
use nanoid::nanoid;
use sea_orm::{Database, DatabaseConnection};
use std::env;
use tower_sessions::{Expiry, SessionManagerLayer, cookie::time::Duration};
use tower_sessions_redis_store::{
    RedisStore,
    fred::prelude::{ClientLike, Config, Pool, Server, ServerConfig},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod argonhasher;
use argonhasher::hash;

use crate::routes::user::user_router;

mod loginsystem;
mod routes;

async fn argon2(Path(password): Path<String>) -> impl IntoResponse {
    let hash = hash(password.as_bytes()).await.unwrap();
    hash
}

async fn nanoid() -> impl IntoResponse {
    nanoid!()
}

async fn root() -> impl IntoResponse {
    "Hello, World!"
}

#[derive(Clone)]
struct AppState {
    db: DatabaseConnection,
}

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
                host: "".into(),
                port: 3003,
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
        .with_expiry(Expiry::OnInactivity(Duration::days(1)));

    // let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    // let db = Database::connect(&database_url).await.unwrap();
    // let _app_state = AppState { db: db };

    let app = Router::new()
        .route("/", get(root))
        .route("/nanoid", get(nanoid))
        .route("/argon2/{password}", get(argon2))
        .nest("/user", user_router());
    // .with_state(_app_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
