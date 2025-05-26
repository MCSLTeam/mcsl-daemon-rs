use anyhow::anyhow;
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Debug, Clone)]
pub enum MinecraftVersion {
    Release(Version),
    Snapshot(String),
    None,
}

impl TryFrom<&str> for MinecraftVersion {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        lazy_static! {
            // 匹配 24w09a 格式（年+周+字母）
            static ref SNAPSHOT_RE: Regex = Regex::new(r"^(\d{2}w\d{2}[a-z])$").unwrap();
        }

        // 优先尝试匹配 Release 版本
        if let Ok(version) = Version::try_from(value) {
            return Ok(MinecraftVersion::Release(version));
        }

        // 然后尝试匹配 Snapshot 版本
        if SNAPSHOT_RE.is_match(value.trim()) {
            return Ok(Self::Snapshot(value.to_string()));
        }

        // 都不匹配则报错
        Err(anyhow!(
            "Invalid version format: '{}'. Expected format examples: 1.20.4 or 24w09a",
            value
        ))
    }
}

use crate::management::version::Version;
use std::cmp::Ordering;

// 手动实现 MinecraftVersion 的比较逻辑
impl PartialEq for MinecraftVersion {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Release(a), Self::Release(b)) => a == b,
            (Self::Snapshot(a), Self::Snapshot(b)) => a == b,
            (Self::None, Self::None) => true,
            _ => false,
        }
    }
}

impl PartialOrd for MinecraftVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Release(a), Self::Release(b)) => a.partial_cmp(b),
            (Self::Snapshot(a), Self::Snapshot(b)) => a.partial_cmp(b),
            (Self::None, Self::None) => Some(Ordering::Equal),
            _ => None, // 不同变体无法比较
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    // 测试版本号解析
    #[test]
    fn test_version_parsing() {
        // 测试合法版本号
        assert_eq!(
            MinecraftVersion::try_from("1.20").unwrap(),
            MinecraftVersion::Release(Version::new(1, 20, None))
        );

        // 测试非法版本号
        assert!(MinecraftVersion::try_from("1.").is_err());
        assert!(MinecraftVersion::try_from("24w9a").is_err());
        assert!(MinecraftVersion::try_from("24W09A").is_err());
    }

    // 测试版本比较逻辑
    #[test]
    fn test_version_comparison() {
        // Release 版本比较
        let v1 = MinecraftVersion::Release(Version::new(1, 20, None));
        let v2 = MinecraftVersion::Release(Version::new(1, 20, Some(4)));
        let v3 = MinecraftVersion::Release(Version::new(1, 19, Some(2)));

        assert!(v1 < v2);
        assert!(v3 < v1);
        assert_eq!(v1.partial_cmp(&v2), Some(Ordering::Less));

        // Snapshot 版本比较（按字母顺序）
        let s1 = MinecraftVersion::Snapshot("24w09a".into());
        let s2 = MinecraftVersion::Snapshot("24w10a".into());
        let s3 = MinecraftVersion::Snapshot("24w09a".into());

        assert!(s1 < s2);
        assert_eq!(s1, s3);
        assert_eq!(s1.partial_cmp(&s2), Some(Ordering::Less));

        // None 的特殊情况
        let n1 = MinecraftVersion::None;
        let n2 = MinecraftVersion::None;
        assert_eq!(n1, n2);
        assert_eq!(n1.partial_cmp(&n2), Some(Ordering::Equal));
    }

    // 测试跨变体比较
    #[test]
    fn test_cross_variant_comparison() {
        let release = MinecraftVersion::Release(Version::new(1, 20, None));
        let snapshot = MinecraftVersion::Snapshot("24w09a".into());
        let none = MinecraftVersion::None;

        // 相等性比较
        assert_ne!(release, snapshot);
        assert_ne!(release, none);
        assert_ne!(snapshot, none);

        // 排序比较应返回 None
        assert_eq!(release.partial_cmp(&snapshot), None);
        assert_eq!(release.partial_cmp(&none), None);
        assert_eq!(snapshot.partial_cmp(&none), None);
    }

    // 测试 Version 结构体本身的比较
    #[test]
    fn test_version_struct_comparison() {
        let v1 = Version::new(1, 20, None);
        let v2 = Version::new(1, 20, Some(4));
        let v3 = Version::new(2, 0, Some(1));

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert_eq!(v1.cmp(&v1), Ordering::Equal);
    }
}
