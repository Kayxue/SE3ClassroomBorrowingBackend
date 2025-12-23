use redis::{Expiry, SetExpiry, SetOptions};
use lazy_static::lazy_static;

pub const REDIS_EXPIRY_SECONDS: u64 = 60;
pub const REDIS_EXPIRY: Expiry = Expiry::EX(REDIS_EXPIRY_SECONDS);

lazy_static! {
    pub static ref REDIS_SET_OPTIONS: SetOptions = SetOptions::default()
        .with_expiration(SetExpiry::EX(REDIS_EXPIRY_SECONDS));
}
