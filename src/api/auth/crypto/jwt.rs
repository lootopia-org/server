use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::utils::contants::{ENC_LABEL, JWS_HEADER, NONCE_LEN, NOW, SIGN_LABEL};

use super::token::random_bytes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub role: String,
    pub sid: String,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Debug)]
pub enum JwtError {
    Malformed,
    Signature,
    Decrypt,
    Expired,
    Crypto,
}

type HmacSha256 = Hmac<Sha256>;

fn derive_key(secret: &[u8], label: &[u8]) -> [u8; 32] {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(label);
    mac.finalize().into_bytes().into()
}

fn sign(secret: &[u8], signing_input: &[u8]) -> Vec<u8> {
    let key = derive_key(secret, SIGN_LABEL);
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(&key).expect("HMAC accepts keys of any length");
    mac.update(signing_input);
    mac.finalize().into_bytes().to_vec()
}

fn cipher(secret: &[u8]) -> Result<Aes256Gcm, JwtError> {
    let key = derive_key(secret, ENC_LABEL);
    <Aes256Gcm as KeyInit>::new_from_slice(&key).map_err(|_| JwtError::Crypto)
}

pub fn encode(secret: &[u8], claims: &Claims) -> Result<String, JwtError> {
    let payload = serde_json::to_vec(claims).map_err(|_| JwtError::Crypto)?;
    let signing_input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(JWS_HEADER),
        URL_SAFE_NO_PAD.encode(&payload)
    );
    let signature = sign(secret, signing_input.as_bytes());
    let jws = format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(signature));

    let nonce_bytes = random_bytes(NONCE_LEN);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher(secret)?
        .encrypt(nonce, jws.as_bytes())
        .map_err(|_| JwtError::Crypto)?;

    Ok(format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(&nonce_bytes),
        URL_SAFE_NO_PAD.encode(&ciphertext)
    ))
}

pub fn decode(secret: &[u8], token: &str) -> Result<Claims, JwtError> {
    let (nonce_b64, ct_b64) = token.split_once('.').ok_or(JwtError::Malformed)?;
    let nonce_bytes = URL_SAFE_NO_PAD
        .decode(nonce_b64)
        .map_err(|_| JwtError::Malformed)?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(ct_b64)
        .map_err(|_| JwtError::Malformed)?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(JwtError::Malformed);
    }

    let nonce = Nonce::from_slice(&nonce_bytes);
    let jws_bytes = cipher(secret)?
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| JwtError::Decrypt)?;
    let jws = String::from_utf8(jws_bytes).map_err(|_| JwtError::Malformed)?;

    let mut parts = jws.split('.');
    let header = parts.next().ok_or(JwtError::Malformed)?;
    let payload = parts.next().ok_or(JwtError::Malformed)?;
    let signature = parts.next().ok_or(JwtError::Malformed)?;
    if parts.next().is_some() {
        return Err(JwtError::Malformed);
    }

    let signing_input = format!("{header}.{payload}");
    let expected = sign(secret, signing_input.as_bytes());
    let provided = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| JwtError::Malformed)?;
    if provided.ct_eq(&expected).unwrap_u8() != 1 {
        return Err(JwtError::Signature);
    }

    let claims: Claims = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| JwtError::Malformed)?,
    )
    .map_err(|_| JwtError::Malformed)?;

    if claims.exp < (*NOW).timestamp() {
        return Err(JwtError::Expired);
    }

    Ok(claims)
}
