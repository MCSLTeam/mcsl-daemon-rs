use anyhow::bail;
use core::str;
use log::debug;
use rusqlite::{
    named_params,
    types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef},
};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// User database : name, secret, password_hash, group, permissions
#[derive(Clone)]
pub struct UserDb {
    conn: Arc<Mutex<Option<rusqlite::Connection>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionGroup {
    Admin,
    User,
    Custom,
}

impl FromSql for PermissionGroup {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Text(text) => match unsafe { str::from_utf8_unchecked(text) } {
                "Admin" => Ok(PermissionGroup::Admin),
                "User" => Ok(PermissionGroup::User),
                "Custom" => Ok(PermissionGroup::Custom),
                _ => Err(FromSqlError::InvalidType),
            },
            _ => Err(FromSqlError::InvalidType),
        }
    }
}
impl ToSql for PermissionGroup {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(match self {
            PermissionGroup::Admin => "Admin",
            PermissionGroup::User => "User",
            PermissionGroup::Custom => "Custom",
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission(String);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Permissions(Vec<Permission>);

impl FromSql for Permissions {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        // use serde_json::from_str;
        match value {
            ValueRef::Text(text) => {
                if let Ok(json) = serde_json::from_str(unsafe { str::from_utf8_unchecked(text) }) {
                    Ok(Permissions(json))
                } else {
                    Err(FromSqlError::InvalidType)
                }
            }
            _ => Err(FromSqlError::InvalidType),
        }
    }
}
impl ToSql for Permissions {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        if let Ok(json) = serde_json::to_string(&self) {
            Ok(ToSqlOutput::from(json))
        } else {
            Err(rusqlite::Error::InvalidQuery)
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserRow {
    pub name: String,
    pub secret: String,
    pub password_hash: String,
    pub group: PermissionGroup,
    pub permissions: Permissions,
}

impl UserDb {
    pub fn new() -> Self {
        Self {
            conn: Arc::new(Mutex::new(None)),
        }
    }
}

// pub trait DbExecutor {
//     fn run<F, T>(conn: &mut Connection, f: F) -> anyhow::Result<T>
//     where
//         F: for<'b> FnOnce(&'b mut Self) -> anyhow::Result<T>;
// }

// impl DbExecutor for Connection {
//     fn run<F, T>(conn: &mut Connection, f: F) -> anyhow::Result<T>
//     where
//         F: FnOnce(&mut Connection) -> anyhow::Result<T>,
//     {
//         f(conn)
//     }
// }

// impl<'a> DbExecutor for Transaction<'a> {
//     fn run<F, T>(conn: &mut Connection, f: F) -> anyhow::Result<T>
//     where
//         F: for<'b> FnOnce(&'b mut Transaction<'a>) -> anyhow::Result<T>,
//     {
//         // 在这里调用 transaction()，编译器将理解生命周期
//         let mut transaction = conn.transaction()?;

//         // 执行闭包
//         let result = f(&mut transaction);

//         // 提交或回滚
//         if result.is_ok() {
//             transaction.commit()?;
//         } else {
//             transaction.rollback()?;
//         }

//         result
//     }
// }

impl UserDb {
    pub async fn open(&self, db: &str) -> anyhow::Result<()> {
        let conn = rusqlite::Connection::open(db)?;

        *self.conn.lock().unwrap() = Some(conn);

        // ensure table
        self.execute_async(|conn| {
            // auto vacuum mode = INCREMENTAL
            conn.pragma_update(None, "auto_vacuum", 1)?;

            conn.execute(
                "CREATE TABLE IF NOT EXISTS users(
                    `name` TEXT PRIMARY KEY,
                    `secret` TEXT,
                    `password_hash` TEXT,
                    `group` TEXT,
                    `permissions` TEXT
                );",
                [],
            )?;
            Ok(())
        })
        .await?;

        Ok(())
    }

    pub fn close(&self) -> anyhow::Result<()> {
        if let Some(conn) = self.conn.lock().unwrap().take() {
            if let Err((_, e)) = conn.close() {
                bail!("Failed to close connection: {}", e);
            }
        }
        Ok(())
    }

    pub async fn lookup(&self, name: &str) -> Option<UserRow> {
        let name_owned = name.to_string();

        let lookup_fn = move |conn: &mut rusqlite::Connection| -> anyhow::Result<UserRow> {
            let mut stmt = conn.prepare("SELECT * FROM users WHERE name = ?;")?;
            let user = stmt.query_row([name_owned], |row| {
                Ok(UserRow {
                    name: row.get(0)?,
                    secret: row.get(1)?,
                    password_hash: row.get(2)?,
                    group: row.get(3)?,
                    permissions: row.get(4)?,
                })
            })?;
            Ok(user)
        };

        match self.execute_async(lookup_fn).await {
            Ok(user) => Some(user),
            Err(e) => {
                debug!("[UserDb] Error looking up user: {:?}", e);
                None
            }
        }
    }

    pub async fn user_rows(&self) -> anyhow::Result<Vec<UserRow>> {
        let rows = self
            .execute_async(|conn| {
                let mut stmt = conn.prepare("SELECT * FROM users;")?;
                let mut rows = vec![];
                stmt.query_map([], |row| {
                    Ok(UserRow {
                        name: row.get(0)?,
                        secret: row.get(1)?,
                        password_hash: row.get(2)?,
                        group: row.get(3)?,
                        permissions: row.get(4)?,
                    })
                })?
                .for_each(|row| {
                    if let Ok(row) = row {
                        rows.push(row);
                    }
                });
                Ok(rows)
            })
            .await?;
        Ok(rows)
    }

    pub async fn has_user(&self, name: &str) -> bool {
        self.lookup(name).await.is_some()
    }

    pub async fn insert_row(&self, user: UserRow) -> anyhow::Result<()> {
        self.execute_async(move |conn| {
            conn.execute(
                "INSERT INTO users (name, secret, password_hash, `group`, permissions) VALUES (?1, ?2, ?3, ?4, ?5);",
                rusqlite::params![user.name, user.secret, user.password_hash, user.group, user.permissions],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn insert(
        &self,
        name: &str,
        secret: &str,
        password_hash: &str,
        group: &PermissionGroup,
        permissions: &Permissions,
    ) -> anyhow::Result<()> {
        let user = UserRow {
            name: name.to_string(),
            secret: secret.to_string(),
            password_hash: password_hash.to_string(),
            group: group.clone(),
            permissions: permissions.clone(),
        };
        self.insert_row(user).await
    }

    pub async fn update(
        &self,
        name: &str,
        secret: Option<String>,
        password_hash: Option<String>,
        group: Option<PermissionGroup>,
        permissions: Option<Permissions>,
    ) -> anyhow::Result<()> {
        let name = name.to_string();
        self.execute_async(move |conn| {
            let mut query = String::from("UPDATE users SET ");
            let mut set_clauses = vec![];

            let mut params = vec![];

            if let Some(ref secret) = secret {
                set_clauses.push("secret = :secret");
                params.push((":secret", secret as &dyn ToSql));
            }
            if let Some(ref password_hash) = password_hash {
                set_clauses.push("password_hash = :password_hash");
                params.push((":password_hash", password_hash as &dyn ToSql));
            }
            if let Some(ref group) = group {
                set_clauses.push("`group` = :group");
                params.push((":group", group as &dyn ToSql));
            }
            if let Some(ref permissions) = permissions {
                set_clauses.push("permissions = :permissions");
                params.push((":permissions", permissions as &dyn ToSql));
            }

            // 连接查询的 SET 部分
            query.push_str(&set_clauses.join(", "));
            query.push_str(" WHERE name = :name");

            // 将 name 参数添加到 params
            params.push((":name", &name as &dyn ToSql));

            let mut stmt = conn.prepare(&query)?;
            stmt.execute(params.as_slice())?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    pub async fn remove(&self, name: &str) -> anyhow::Result<()> {
        let name = name.to_string();
        self.execute_async(move |conn| {
            let mut stmt = conn.prepare("DELETE FROM users WHERE name = :name")?;
            stmt.execute(named_params! {
                ":name": name
            })?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    async fn execute_async<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut rusqlite::Connection) -> anyhow::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        // Clone the Arc to share it with the async task
        let conn_arc = Arc::clone(&self.conn);
        // Spawn a new task to execute the provided function
        let result = tokio::task::spawn_blocking(move || {
            // Lock the mutex and get a mutable reference to the connection
            let mut conn = conn_arc.lock().unwrap(); // Handle lock errors as needed

            // Call the provided function with the mutable reference to the connection
            if let Some(conn) = conn.as_mut() {
                f(conn)
            } else {
                bail!("Connection is not open")
            }
        })
        .await?;

        // Return the result
        result.map_err(Into::into) // Convert rusqlite errors to anyhow::Error
    }
}

impl Drop for UserDb {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
