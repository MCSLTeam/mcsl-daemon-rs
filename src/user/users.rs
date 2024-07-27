use std::num::NonZeroU32;
use dashmap::DashMap;
use ring::pbkdf2;
use ring::pbkdf2::{Algorithm, PBKDF2_HMAC_SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use crate::base64::{base64_decode,base64_encode};

const SALT_LEN: usize = 16;
const PWD_HASH_LEN: usize = 32;
const ITERATIONS: u32 = 10000;
static PBKDF2_ALGO: Algorithm = PBKDF2_HMAC_SHA256;

pub enum PermissionGroups {
    Admin,
    Users,
    Custom,
}

pub struct Permission(String);

pub struct UserMeta {
    pwd_hash: String,
    permission_groups: PermissionGroups,
    permissions: Vec<Permission>,
}
pub struct Users {
    users: DashMap<String, UserMeta>,
}

impl Users {
    pub fn authenticate(&self, usr: &str, pwd: &str) -> Option<UserMeta> {
        self.users.get(usr).and_then(|user| {
            if Self::verify_pwd(pwd, &user.pwd_hash) {
                Some(user.value().clone().into())
            } else {
                None
            }
        })
    }

    fn verify_pwd(pwd: &str, pwd_hash: &str) -> bool {
        if let Ok(pwd_hash) = base64_decode(pwd_hash) {
            let mut output = [0; PWD_HASH_LEN];
            if let Ok(_) = pbkdf2::verify(PBKDF2_ALGO, NonZeroU32::try_from(10000).unwrap(), &pwd_hash, pwd.as_bytes(), &mut output) {
                return true;
            }
        }
        false
    }

    // 使用Pbkdf2,盐量16,key长32,迭代次数10000，hash算法：sha256
    fn hash_pwd(pwd: &str) -> String {
        let rng = SystemRandom::new();
        let mut salt = [0; SALT_LEN];
        rng.fill(&mut salt).expect("Failed to generate salt");

        let mut output = [0; PWD_HASH_LEN];
        pbkdf2::derive(PBKDF2_HMAC_SHA256, NonZeroU32::try_from(10000).unwrap(), &salt, pwd.as_bytes(), &mut output);
        base64_encode(&output)
    }
}