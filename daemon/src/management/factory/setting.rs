use crate::management::config::InstanceConfigExt;
use anyhow::Context;
use mcsl_protocol::management::instance::InstanceFactorySetting;

mod setting_utils {
    use crate::management::config::InstanceConfigExt;
    use anyhow::bail;
    use mcsl_protocol::management::instance::InstanceFactorySetting;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use url::Url;

    pub async fn ensure_source(setting: &InstanceFactorySetting) -> anyhow::Result<PathBuf> {
        let working_dir = setting.config.get_working_dir();
        let source_path = match Url::parse(setting.source.as_str()) {
            Ok(url) => match url.scheme() {
                "file" => {
                    let path = match url.to_file_path() {
                        Ok(path) => path,
                        Err(_) => {
                            bail!("invalid url: {}", url)
                        }
                    };
                    if path.starts_with(&working_dir) {
                        path
                    } else {
                        bail!("invalid file url: {}", working_dir.as_path().display())
                    }
                }
                "http" | "https | ftp" | "ftps" => {
                    todo!("支持下载网络Source")
                }
                _ => {
                    bail!("source with unsupported url scheme: {}", url)
                }
            },
            Err(_) => working_dir.join(setting.source.as_str()),
        };
        Ok(working_dir.join(source_path))
    }

    pub fn generate_eula(path: impl AsRef<Path>) -> anyhow::Result<()> {
        let mut eula = std::fs::File::open(path.as_ref())?;
        eula.write_all(b"#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).")?;
        eula.write_all(
            format!(
                "#{}\n",
                chrono::Local::now().format("%a %b %d %H:%M:%S %Z %Y")
            )
            .as_bytes(),
        )?;
        eula.write_all(b"eula=true")?;
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait InstanceFactorySettingExt {
    async fn fix_eula(&self) -> anyhow::Result<()>;
    async fn copy_and_rename_target(&self) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl InstanceFactorySettingExt for InstanceFactorySetting {
    async fn fix_eula(&self) -> anyhow::Result<()> {
        let eula_path = self.config.get_working_dir().join("eula.txt");
        if eula_path.exists() {
            let content = tokio::fs::read_to_string(&eula_path).await?;
            let eula = content
                .lines()
                .map(|l| {
                    if l.starts_with("eula") {
                        "eula=true"
                    } else {
                        l
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            tokio::fs::write(&eula_path, eula.as_bytes()).await?;
            Ok(())
        } else {
            setting_utils::generate_eula(eula_path.as_path())
        }
    }
    async fn copy_and_rename_target(&self) -> anyhow::Result<()> {
        let working_dir = self.config.get_working_dir();
        let source_path = setting_utils::ensure_source(self)
            .await
            .context("failed to ensure source")?;
        let target_path = working_dir.join(self.source.as_str());

        if source_path.as_path() != target_path.as_path() {
            tokio::fs::rename(source_path.as_path(), target_path.as_path())
                .await
                .context("failed to rename source")?;
        }

        Ok(())
    }
}
