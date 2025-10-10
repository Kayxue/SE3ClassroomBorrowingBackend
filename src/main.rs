use std::{net::SocketAddr, sync::Arc};

use axum::{Router, extract::Path, response::IntoResponse, routing::get};
use dotenv::dotenv;
use nanoid::nanoid;
use sea_orm::DatabaseConnection;
use std::env;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod argonhasher;
use argonhasher::{Config, hash};

mod loginsystem;

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
struct AppState{
    db: Arc<DatabaseConnection>
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

    let password_hashing_secret: String = env::var("PASSWORD_HASHING_SECRET").unwrap();

    let argon2_config = Config {
        iterations: 4,
        parallelism: 4,
        memory_cost: 512,
        secret_key: password_hashing_secret.as_bytes().to_vec(),
    };

    argonhasher::set_config(argon2_config);

    // let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    // let db = Arc::new(sea_orm::Database::connect(&database_url).await.unwrap());
    // let _app_state = AppState { db };

    let app = Router::new()
        .route("/", get(root))
        .route("/nanoid", get(nanoid))
        .route("/argon2/{password}", get(argon2));
        // .with_state(_app_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
