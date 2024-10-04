use std::collections::HashMap;

use crate::user::{
    auth::Auth,
    userdb::{PermissionGroup, Permissions, UserDb},
};
use crate::utils;
use anyhow::bail;
use log::info;
use serde::{Deserialize, Serialize};

use super::JwtClaims;

pub trait UsersManager: Sync {
    async fn auth(&self, usr: &str, pwd: &str) -> Option<UserMeta>;
    async fn auth_token(&self, token: &str) -> Option<User>;
    async fn gen_token(&self, usr: &str, expired: u64) -> anyhow::Result<String>;

    async fn add_user(&self, usr: &str, meta: &UserMeta) -> anyhow::Result<()>;
    async fn remove_user(&self, usr: &str) -> anyhow::Result<()>;
    async fn change_pwd(&self, usr: &str, pwd: &str) -> anyhow::Result<()>;
    async fn get_user_meta(&self, usr: &str) -> Option<UserMeta>;
    async fn get_users(&self) -> anyhow::Result<HashMap<String, UserMeta>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMeta {
    pub secret: String,
    pub pwd_hash: String,
    pub permission_groups: PermissionGroup,
    pub permissions: Permissions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub usr: String,
    pub meta: UserMeta,
}

pub struct Users {
    user_db: UserDb,
}

impl UsersManager for Users {
    async fn auth(&self, usr: &str, pwd: &str) -> Option<UserMeta> {
        self.user_db.lookup(usr).await.and_then(|user_row| {
            if Auth::verify_pwd(pwd, &user_row.password_hash) {
                Some(UserMeta {
                    secret: user_row.secret,
                    pwd_hash: user_row.password_hash,
                    permission_groups: user_row.group,
                    permissions: user_row.permissions,
                })
            } else {
                None
            }
        })
    }

    async fn auth_token(&self, token: &str) -> Option<User> {
        if let Some(name) = JwtClaims::extract_usr(token) {
            // try get user token secret
            let user_query = self.user_db.lookup(&name).await;
            if let Some(secret) = user_query.as_ref().and_then(|ref row| Some(&row.secret)) {
                // validate token
                return JwtClaims::from_token(token, secret)
                    .ok()
                    .and_then(|claims| {
                        let user_row = user_query.unwrap(); // unwrap is safe
                        if &user_row.name == &claims.usr {
                            Some(User {
                                usr: user_row.name,
                                meta: UserMeta {
                                    secret: user_row.secret,
                                    pwd_hash: user_row.password_hash,
                                    permission_groups: user_row.group,
                                    permissions: user_row.permissions,
                                },
                            })
                        } else {
                            // a very confusing error, query ok but user name not match
                            None
                        }
                    });
            }
        }
        None
    }

    async fn gen_token(&self, usr: &str, expired: u64) -> anyhow::Result<String> {
        if let Some(user_row) = self.user_db.lookup(usr).await {
            let claims = JwtClaims::new(user_row.name, expired);
            Ok(claims.to_token(&user_row.secret))
        } else {
            bail!("[Users] Could not generate token")
        }
    }

    async fn add_user(&self, usr: &str, meta: &UserMeta) -> anyhow::Result<()> {
        if self.user_db.has_user(usr).await {
            bail!("User already exists")
        }
        self.user_db
            .insert(
                usr,
                &meta.secret,
                &meta.pwd_hash,
                &meta.permission_groups,
                &meta.permissions,
            )
            .await?;
        Ok(())
    }

    async fn remove_user(&self, usr: &str) -> anyhow::Result<()> {
        self.user_db.remove(usr).await?;
        Ok(())
    }

    async fn change_pwd(&self, usr: &str, pwd: &str) -> anyhow::Result<()> {
        if self.user_db.has_user(usr).await {
            // expire tokens
            self.expire_user_tokens(usr).await?;
            self.user_db
                .update(usr, None, Some(Auth::hash_pwd(pwd)), None, None)
                .await?;
        } else {
            bail!("User not found")
        }
        Ok(())
    }

    async fn get_user_meta(&self, usr: &str) -> Option<UserMeta> {
        if let Some(user) = self.user_db.lookup(usr).await {
            Some(UserMeta {
                secret: user.secret,
                pwd_hash: user.password_hash,
                permission_groups: user.group,
                permissions: user.permissions,
            })
        } else {
            None
        }
    }

    async fn get_users(&self) -> anyhow::Result<HashMap<String, UserMeta>> {
        Ok(self
            .user_db
            .user_rows()
            .await?
            .into_iter()
            .map(|user_row| {
                (
                    user_row.name,
                    UserMeta {
                        secret: user_row.secret,
                        pwd_hash: user_row.password_hash,
                        permission_groups: user_row.group,
                        permissions: user_row.permissions,
                    },
                )
            })
            .collect::<HashMap<_, _>>())
    }
}

impl Users {
    fn new() -> Self {
        // DashMap 添加了serde feature可以直接序列化反序列化
        Self {
            user_db: UserDb::new(),
        }
    }

    pub async fn build(db_path: &'static str) -> anyhow::Result<Self> {
        let this = Self::new();

        this.user_db.open(db_path).await?;

        Ok(this)
    }

    pub async fn fix_admin(&self) -> anyhow::Result<()> {
        if !self.user_db.has_user("admin").await {
            let random_pwd = utils::get_random_string(16);
            info!(
                " [Users] *** generate admin account: name=admin, pwd={}",
                random_pwd
            );
            self.add_user(
                "admin",
                &UserMeta {
                    secret: utils::get_random_string(16),
                    pwd_hash: Auth::hash_pwd(&random_pwd),
                    permission_groups: PermissionGroup::Admin,
                    permissions: Permissions::default(),
                },
            )
            .await?;
        }
        Ok(())
    }

    pub async fn expire_user_tokens(&self, usr: &str) -> anyhow::Result<()> {
        if self.user_db.has_user(usr).await {
            let new_secret = utils::get_random_string(16);
            // change secret to expire user tokens
            self.user_db
                .update(usr, Some(new_secret), None, None, None)
                .await?;
        }
        Ok(())
    }
}
