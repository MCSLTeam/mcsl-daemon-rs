use std::borrow::Cow;
use serde::{Deserialize, Serialize};
use crate::auth::jwt::generate_secret_string;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: Cow<'static, str>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig {
            jwt_secret: Cow::Owned(generate_secret_string(32).unwrap()),
        }
    }
}
