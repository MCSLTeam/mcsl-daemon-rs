use std::num::NonZeroU32;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::pbkdf2;
use ring::pbkdf2::PBKDF2_HMAC_SHA256;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

use crate::base64;

const SALT_LEN: usize = 16;
const CREDENTIAL_LEN: usize = 32;
const N_ITER: u32 = 10_000;
pub struct Auth;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct JwtClaims {
    exp: u64,
    leeway: u64,
    iss: String,
    aud: String,
    usr: String,
    pwd: String,
}

impl JwtClaims {
    pub fn new(usr: String, pwd: String, exp: u64) -> Self {
        Self {
            exp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + exp,
            leeway: 0,
            iss: "MCServerLauncher.Daemon".to_string(),
            aud: "MCServerLauncher.Daemon".to_string(),
            usr,
            pwd,
        }
    }
}

impl Auth {
    pub fn verify_pwd(pwd: &str, pwd_hash: &str) -> bool {
        let parts: Vec<&str> = pwd_hash.split('$').collect();
        if parts.len() != 2 {
            return false;
        }

        let salt = base64::decode(parts[0]).unwrap();
        let stored_hash = base64::decode(parts[1]).unwrap();


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

        let salt_base64 = base64::encode(&salt);
        let hash_base64 = base64::encode(&pbkdf2_hash);
        format!("{}${}", salt_base64, hash_base64)
    }
}