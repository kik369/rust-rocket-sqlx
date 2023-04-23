extern crate bcrypt;

use bcrypt::{hash, verify, DEFAULT_COST};
/// hash password
pub fn hash_password(password: &str) -> String {
    hash(password, DEFAULT_COST).expect("failed to hash password")
}

/// verify password
pub fn verify_password(password: &str, hashed_password: &str) -> bool {
    verify(password, hashed_password).expect("failed to verify password")
}
