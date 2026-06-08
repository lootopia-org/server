use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use subtle::ConstantTimeEq;

const SALT_LENGTH: usize = 16;
const KEY_LENGTH: usize = 32;

#[derive(Debug, Clone)]
pub struct PasswordParams {
    pub pepper: Vec<u8>,
    pub iterations: u32,
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPassword {
    pub salt: Vec<u8>,
    pub hash: Vec<u8>,
}

fn peppered(pepper: &[u8], password: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(pepper)
        .expect("HMAC accepts keys of any length");
    mac.update(password.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn derive(params: &PasswordParams, salt: &[u8], pw_input: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; KEY_LENGTH];
    pbkdf2::pbkdf2_hmac::<Sha256>(pw_input, salt, params.iterations, &mut out);
    out
}

pub fn hash_new_password(params: &PasswordParams, password: &str) -> StoredPassword {
    let mut salt = vec![0u8; SALT_LENGTH];
    rand::rng().fill_bytes(&mut salt);
    let pw_input = peppered(&params.pepper, password);
    let hash = derive(params, &salt, &pw_input);
    StoredPassword { salt, hash }
}

pub fn verify_password(params: &PasswordParams, password: &str, stored: &StoredPassword) -> bool {
    let pw_input = peppered(&params.pepper, password);
    let candidate = derive(params, &stored.salt, &pw_input);
    candidate.ct_eq(&stored.hash).into()
}
