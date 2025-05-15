use crate::auth::jwt::generate_secret_string;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub secret: Cow<'static, str>,
    pub main_token: Cow<'static, str>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig {
            secret: Cow::Owned(generate_secret_string(32).unwrap()),
            main_token: Cow::Owned(generate_secret_string(32).unwrap()),
        }
    }
}
