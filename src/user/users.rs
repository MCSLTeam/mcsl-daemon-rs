use std::path::Path;

use anyhow::{bail, Context};
use dashmap::DashMap;
use log::info;
use serde::{Deserialize, Serialize};

use crate::storage::file::{Config, FileIoWithBackup};
use crate::user::auth::Auth;
use crate::utils;

pub trait UsersManager: Sync {
    fn authenticate(&self, usr: &str, pwd: &str) -> Option<UserMeta>;
    fn add_user(
        &self,
        usr: &str,
        pwd: &str,
        groups: PermissionGroups,
        permissions: Vec<Permission>,
    ) -> anyhow::Result<()>;
    fn remove_user(&self, usr: &str) -> anyhow::Result<()>;
    fn change_pwd(&self, usr: &str, pwd: &str) -> anyhow::Result<()>;
    fn get_user_meta(&self, usr: &str) -> Option<UserMeta>;
    fn get_user_list(&self) -> Vec<String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionGroups {
    Admin,
    Users,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission(String);

#[derive(Clone, Serialize, Deserialize)]
pub struct UserMeta {
    pub pwd_hash: String,
    pub permission_groups: PermissionGroups,
    pub permissions: Vec<Permission>,
}

#[derive(Serialize, Deserialize)]
pub struct Users {
    users: DashMap<String, UserMeta>,

    #[serde(skip)]
    config_path: &'static str,
}

impl FileIoWithBackup for Users {}

impl Config for Users {
    type ConfigType = Users;
}

impl UsersManager for Users {
    fn authenticate(&self, usr: &str, pwd: &str) -> Option<UserMeta> {
        self.users.get(usr).and_then(|user| {
            if Auth::verify_pwd(pwd, &user.pwd_hash) {
                Some(user.value().clone())
            } else {
                None
            }
        })
    }

    fn add_user(
        &self,
        usr: &str,
        pwd: &str,
        group: PermissionGroups,
        permissions: Vec<Permission>,
    ) -> anyhow::Result<()> {
        if self.users.contains_key(usr) {
            bail!("User already exists")
        }
        self.users.insert(
            usr.to_string(),
            UserMeta {
                pwd_hash: Auth::hash_pwd(pwd),
                permission_groups: group,
                permissions,
            },
        );
        Ok(())
    }

    fn remove_user(&self, usr: &str) -> anyhow::Result<()> {
        if self.users.remove(usr).is_none() {
            bail!("User not found")
        }
        Ok(())
    }

    fn change_pwd(&self, usr: &str, pwd: &str) -> anyhow::Result<()> {
        if self.users.contains_key(usr) {
            self.users.insert(
                usr.to_string(),
                UserMeta {
                    pwd_hash: Auth::hash_pwd(pwd),
                    permission_groups: PermissionGroups::Users,
                    permissions: vec![],
                },
            );
            Ok(())
        } else {
            bail!("User not found")
        }
    }

    fn get_user_meta(&self, usr: &str) -> Option<UserMeta> {
        self.users.get(usr).map(|user| user.value().clone())
    }

    fn get_user_list(&self) -> Vec<String> {
        self.users.iter().map(|user| user.key().clone()).collect()
    }
}

impl Users {
    pub fn new(config_path: &'static str) -> Self {
        // DashMap 添加了serde feature可以直接序列化反序列化
        let path = Path::new(config_path);
        Self::load_config_or_default(path, || {
            let users = DashMap::new();
            Users { users, config_path }
        })
        .unwrap()
    }

    pub fn fix_admin(&self) -> anyhow::Result<()> {
        if self.users.get("admin").is_none() {
            let random_pwd = utils::get_random_string(16);
            info!(
                " [Users] *** generate admin account: name=admin, pwd={}",
                random_pwd
            );
            self.users.insert(
                "admin".to_string(),
                UserMeta {
                    pwd_hash: Auth::hash_pwd(&random_pwd),
                    permission_groups: PermissionGroups::Admin,
                    permissions: vec![],
                },
            );
            Self::save_config(self.config_path, self).context("Failed to save config")?
        }
        Ok(())
    }
}
