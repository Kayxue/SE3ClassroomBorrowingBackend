use redis::{Expiry, SetExpiry, SetOptions};

pub const REDIS_EXPIRY_SECONDS: u64 = 60;
pub const REDIS_EXPIRY: Expiry = Expiry::EX(REDIS_EXPIRY_SECONDS);

pub fn get_redis_set_options() -> SetOptions {
    SetOptions::default()
        .with_expiration(SetExpiry::EX(REDIS_EXPIRY_SECONDS))
}
