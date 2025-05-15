use crate::config::AppConfig;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const CHARS_LEN: usize = CHARS.len();

pub fn generate_secret_string(length: usize) -> Result<String, ring::error::Unspecified> {
    let rng = SystemRandom::new();
    let mut s = String::with_capacity(length);

    for _ in 0..length {
        let idx = uniform_random_index(&rng, CHARS_LEN)?;
        s.push(CHARS[idx] as char);
    }

    Ok(s)
}

fn uniform_random_index(rng: &SystemRandom, max: usize) -> Result<usize, ring::error::Unspecified> {
    let byte_count = ((max as f64).log2() / 8.0).ceil() as usize;
    let mut buf = vec![0u8; byte_count];

    loop {
        rng.fill(&mut buf)?;
        let num = buf.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64);
        if num <= (u64::MAX - (u64::MAX % max as u64)) {
            return Ok((num % max as u64) as usize);
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct JwtClaims {
    iss: String,
    aud: String,
    pub exp: u64,
    pub jti: String,
    pub perms: String,
}

impl JwtClaims {
    pub fn new(exp: u64, perms: String) -> Self {
        Self {
            exp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + exp,
            iss: "MCServerLauncher.Daemon".into(),
            aud: "MCServerLauncher.Daemon".into(),
            jti: uuid::Uuid::new_v4().to_string(),
            perms,
        }
    }
}

pub trait JwtCodec: Serialize + for<'de> Deserialize<'de> {
    fn from_token(token: &str) -> Result<Self, jsonwebtoken::errors::Error>;
    fn to_token(&self) -> String;
}

impl JwtCodec for JwtClaims {
    fn from_token(token: &str) -> Result<Self, jsonwebtoken::errors::Error> {
        let mut validation = Validation::default();
        validation.set_audience(&["MCServerLauncher.Daemon".to_string()]);
        validation.set_issuer(&["MCServerLauncher.Daemon".to_string()]);
        validation.leeway = 0;

        decode::<Self>(
            token,
            &DecodingKey::from_secret(AppConfig::get().auth.secret.as_bytes()),
            &validation,
        )
        .map(|data| data.claims)
    }

    fn to_token(&self) -> String {
        encode(
            &Header::default(),
            &self,
            &EncodingKey::from_secret(AppConfig::get().auth.secret.as_bytes()),
        )
        .unwrap()
    }
}
