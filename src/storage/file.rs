use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub trait FileIoWithBackup {
    /// Writes the given content to a file and creates a backup of the file before writing.
    fn write_with_backup<P: AsRef<Path>>(path: P, content: &str) -> Result<(), std::io::Error> {
        let path = path.as_ref();

        if path.exists() {
            let backup_path = path.with_extension("bak");

            // Create a backup of the file
            fs::copy(path, backup_path)?;
        }

        // Write the content to the file
        fs::write(path, content)?;

        Ok(())
    }
}

/// Trait for configuration handling.
pub trait Config: FileIoWithBackup {
    type ConfigType: Serialize + for<'de> Deserialize<'de>;

    fn load_config<P: AsRef<Path>>(path: P) -> anyhow::Result<Self::ConfigType> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;
        let config: Self::ConfigType = serde_json::from_str(&content)?;
        Ok(config)
    }

    fn save_config<P: AsRef<Path>>(path: P, config: &Self::ConfigType) -> anyhow::Result<()> {
        let path = path.as_ref();
        let content = serde_json::to_string_pretty(config)?;
        Self::write_with_backup(path, &content)?;
        Ok(())
    }

    fn load_config_or_default<P: AsRef<Path>, F: FnOnce() -> Self::ConfigType>(
        path: P,
        default: F,
    ) -> anyhow::Result<Self::ConfigType> {
        match fs::metadata(path.as_ref()) {
            Ok(metadata) if metadata.is_file() => Self::load_config(path),
            _ => {
                let config = default();
                Self::save_config(path, &config)?;
                Ok(config)
            }
        }
    }
}
