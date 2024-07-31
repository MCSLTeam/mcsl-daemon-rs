use std::num::NonZeroU32;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{decode, DecodingKey, encode, EncodingKey, errors, Header, Validation};
use ring::pbkdf2;
use ring::pbkdf2::PBKDF2_HMAC_SHA256;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

use crate::utils::{base64_decode, base64_encode};

const SALT_LEN: usize = 16;
const CREDENTIAL_LEN: usize = 32;
const N_ITER: u32 = 10_000;
pub struct Auth;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct JwtClaims {
    exp: u64,
    iss: String,
    aud: String,
    pub usr: String,
    pub pwd: String,
}

impl JwtClaims {
    pub fn new(usr: String, pwd: String, exp: u64) -> Self {
        Self {
            exp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + exp,
            iss: "MCServerLauncher.Daemon".to_string(),
            aud: "MCServerLauncher.Daemon".to_string(),
            usr,
            pwd,
        }
    }

    pub fn from_token(token: &str, secret: &str) -> Result<Self, errors::Error> {
        let mut validation = Validation::default();
        validation.set_audience(&["MCServerLauncher.Daemon".to_string()]);
        validation.set_issuer(&["MCServerLauncher.Daemon".to_string()]);
        validation.leeway = 0;

        decode::<Self>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        ).map(|data| data.claims)
    }
    pub fn to_token(&self, secret: &str) -> String {
        encode(
            &Header::default(),
            &self,
            &EncodingKey::from_secret(secret.as_bytes()),
        ).unwrap()
    }
}

impl Auth {
    pub fn verify_pwd(pwd: &str, pwd_hash: &str) -> bool {
        let parts: Vec<&str> = pwd_hash.split('$').collect();
        if parts.len() != 2 {
            return false;
        }

        let salt = base64_decode(parts[0]).unwrap();
        let stored_hash = base64_decode(parts[1]).unwrap();


        pbkdf2::verify(
            PBKDF2_HMAC_SHA256,
            NonZeroU32::new(N_ITER).unwrap(),
            &salt,
            pwd.as_bytes(),
            &stored_hash,
        ).is_ok()
    }

    // 使用Pbkdf2,盐量16,key长32,迭代次数10000，hash算法：sha256
    pub fn hash_pwd(pwd: &str) -> String {
        let rng = SystemRandom::new();
        let mut salt = [0u8; SALT_LEN];
        rng.fill(&mut salt).map_err(|e| e.to_string()).unwrap();

        let mut pbkdf2_hash = [0u8; CREDENTIAL_LEN];
        pbkdf2::derive(
            PBKDF2_HMAC_SHA256,
            NonZeroU32::new(N_ITER).unwrap(),
            &salt,
            pwd.as_bytes(),
            &mut pbkdf2_hash,
        );

        let salt_base64 = base64_encode(&salt);
        let hash_base64 = base64_encode(&pbkdf2_hash);
        format!("{}${}", salt_base64, hash_base64)
    }
}