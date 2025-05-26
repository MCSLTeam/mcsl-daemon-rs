use anyhow::{anyhow, bail};
use lazy_static::lazy_static;
use regex::Regex;
use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub struct Version {
    major: u8,
    minor: u8,
    patch: Option<u8>,
}

impl Version {
    pub fn new(major: u8, minor: u8, patch: Option<u8>) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl TryFrom<&str> for Version {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        lazy_static! {
            // 匹配 1.20 或 1.20.4 格式
            static ref RELEASE_RE: Regex = Regex::new(r"^(\d+)\.(\d+)(?:\.(\d+))?$").unwrap();
        }
        if let Some(caps) = RELEASE_RE.captures(value.trim()) {
            let major = caps[1]
                .parse::<u8>()
                .map_err(|_| anyhow!("Invalid major version: '{}'", &caps[1]))?;

            let minor = caps[2]
                .parse::<u8>()
                .map_err(|_| anyhow!("Invalid minor version: '{}'", &caps[2]))?;

            let patch = caps
                .get(3)
                .map(|m| {
                    m.as_str()
                        .parse::<u8>()
                        .map_err(|_| anyhow!("Invalid patch version: '{}'", m.as_str()))
                })
                .transpose()?;
            Ok(Self {
                major,
                minor,
                patch,
            })
        } else {
            bail!("Invalid version: '{}'", &value)
        }
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major
            && self.minor == other.minor
            && self.patch.unwrap_or(0) == other.patch.unwrap_or(0)
    }
}

// 实现全比较（PartialEq + Eq）
impl Eq for Version {}

// 实现排序比较
impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => self.patch.unwrap_or(0).cmp(&other.patch.unwrap_or(0)),
                ordering => ordering,
            },
            ordering => ordering,
        }
    }
}

// 实现部分排序（为了兼容性）
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        let v1 = Version::new(1, 20, None);
        let v2 = Version::new(1, 20, Some(4));
        let v3 = Version::new(1, 20, Some(0));
        assert!(v1 < v2);
        assert_eq!(v1, v3);
    }
}
