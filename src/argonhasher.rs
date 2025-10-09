use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::{self, SaltString, rand_core::OsRng},
};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::task;

static GLOBAL_ARGON2: OnceLock<Arc<Argon2<'static>>> = OnceLock::new();

pub struct Config {
    pub secret_key: Vec<u8>,
    pub iterations: u32,
    pub parallelism: u32,
    pub memory_cost: u32,
}

pub fn set_config(config: Config) {
    let secret_bytes: &'static [u8] = Box::leak(config.secret_key.into_boxed_slice());

    let argon2 = Argon2::new_with_secret(
        secret_bytes,
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(
            config.memory_cost,
            config.iterations,
            config.parallelism,
            None,
        )
        .unwrap(),
    )
    .unwrap();

    let _ = GLOBAL_ARGON2.set(Arc::new(argon2));
}

pub async fn hash(password: impl AsRef<[u8]>) -> Result<String, password_hash::Error> {
    let argon2 = GLOBAL_ARGON2
        .get()
        .expect("Argon2 instance not initialized. Call set_config first.")
        .clone();

    let password = password.as_ref().to_owned();

    let res = task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        argon2
            .hash_password(&password, &salt)
            .map(|ph| ph.to_string())
    });

    res.await.unwrap()
}

pub async fn verify(
    password: impl AsRef<[u8]>,
    hash: impl AsRef<str>,
) -> Result<bool, password_hash::Error> {
    let argon2 = GLOBAL_ARGON2
        .get()
        .expect("Argon2 instance not initialized. Call set_config first.")
        .clone();
    let password = password.as_ref().to_owned();
    let hash = hash.as_ref().to_owned();

    let res = task::spawn_blocking(move || {
        let hash = PasswordHash::new(&hash).unwrap();
        argon2.verify_password(&password, &hash).map(|_| true)
    });

    res.await.unwrap()
}
