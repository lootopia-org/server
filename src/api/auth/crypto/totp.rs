use std::time::{SystemTime, UNIX_EPOCH};

use base32::Alphabet;
use hmac::{Hmac, Mac};
use sha1::Sha1;

const PERIOD: u64 = 30;
const DIGITS: u32 = 6;
const SECRET_LENGTH: usize = 20;

pub fn generate_secret() -> Vec<u8> {
    super::token::random_bytes(SECRET_LENGTH)
}

pub fn secret_to_base32(secret: &[u8]) -> String {
    base32::encode(Alphabet::Rfc4648 { padding: false }, secret)
}

pub fn otpauth_uri(issuer: &str, account: &str, secret: &[u8]) -> String {
    let enc = |s: &str| -> String {
        s.chars()
            .map(|c| match c {
                ' ' => "%20".to_string(),
                '/' => "%2F".to_string(),
                ':' => "%3A".to_string(),
                '?' => "%3F".to_string(),
                '&' => "%26".to_string(),
                '=' => "%3D".to_string(),
                other => other.to_string(),
            })
            .collect()
    };
    format!(
        "otpauth://totp/{issuer}:{account}?secret={secret}&issuer={issuer}&algorithm=SHA1&digits={digits}&period={period}",
        issuer = enc(issuer),
        account = enc(account),
        secret = secret_to_base32(secret),
        digits = DIGITS,
        period = PERIOD,
    )
}

fn hotp(secret: &[u8], counter: u64) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();

    let offset = (digest[19] & 0x0f) as usize;
    let bin_code = ((u32::from(digest[offset]) & 0x7f) << 24)
        | ((u32::from(digest[offset + 1]) & 0xff) << 16)
        | ((u32::from(digest[offset + 2]) & 0xff) << 8)
        | (u32::from(digest[offset + 3]) & 0xff);
    let value = bin_code % 10u32.pow(DIGITS);
    format!("{value:0width$}", width = DIGITS as usize)
}

pub fn verify_code(secret: &[u8], code: &str) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let counter = now / PERIOD;
    let candidate = code.trim();
    [counter.wrapping_sub(1), counter, counter + 1]
        .iter()
        .any(|c| hotp(secret, *c) == candidate)
}
