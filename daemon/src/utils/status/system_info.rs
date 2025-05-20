use anyhow::{anyhow, bail, Result};
use mcsl_protocol::status::{CpuInfo, DriveInfo, MemInfo, OsInfo, SysInfo};
use std::path::{Path, PathBuf};
use sysinfo::{Cpu, CpuRefreshKind, Disks, System};

// 实现部分
pub async fn get_sys_info() -> Result<SysInfo> {
    let os = get_os_info();
    let cpu = get_cpu_info().await?;
    let mem = get_mem_info();
    let drive = get_disk_info()?;

    Ok(SysInfo {
        os,
        cpu,
        mem,
        drive,
    })
}
pub fn get_os_info() -> OsInfo {
    OsInfo {
        name: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

pub async fn get_cpu_info() -> Result<CpuInfo> {
    let mut system = System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()),
    );

    system.refresh_cpu_specifics(CpuRefreshKind::everything());
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let cpus: Vec<&Cpu> = system.cpus().iter().collect();
    if cpus.is_empty() {
        bail!("No CPU information available")
    }

    let vendor = cpus[0].vendor_id().to_string();
    let name = cpus[0].brand().to_string();
    let count = cpus.len() as u32;

    system.refresh_cpu_usage();
    let usage = system.global_cpu_usage();

    Ok(CpuInfo {
        vendor,
        name,
        count,
        usage,
    })
}
pub fn get_mem_info() -> MemInfo {
    let mut sys = System::new();
    sys.refresh_memory();

    MemInfo {
        total: sys.total_memory() / 1024,
        free: sys.available_memory() / 1024,
    }
}

pub fn get_disk_info() -> Result<DriveInfo> {
    let location =
        std::env::current_exe().map_err(|e| anyhow!("Failed to get executable path: {}", e))?;

    let root_path = get_root_path(&location)?;
    let normalized_root = normalize_drive_name(&root_path);

    let disks = Disks::new_with_refreshed_list();
    let drive = disks
        .into_iter()
        .map(|disk| {
            let name = normalize_drive_name(disk.mount_point());
            (disk, name)
        })
        .find(|(_, name)| is_path_match(name, &normalized_root))
        .map(|(disk, _)| disk);

    let drive = drive.ok_or_else(|| anyhow!("No drive found for root: {}", root_path.display()))?;

    Ok(DriveInfo {
        drive_format: drive.file_system().to_string_lossy().to_string(),
        total: drive.total_space(),
        free: drive.available_space(),
    })
}

pub fn get_root_path(path: &Path) -> Result<PathBuf> {
    let root = path
        .ancestors()
        .find(|p| p.parent().is_none())
        .ok_or_else(|| anyhow!("Cannot determine root path"))?;

    #[cfg(not(windows))]
    {
        if root == Path::new("") {
            return Ok(PathBuf::from("/"));
        }
    }

    Ok(root.to_path_buf())
}

fn normalize_drive_name<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref();
    let normalized = path
        .to_str()
        .unwrap_or("")
        .trim_end_matches(std::path::MAIN_SEPARATOR);

    #[cfg(not(windows))]
    {
        if normalized.is_empty() {
            return "/".to_string();
        }
    }

    normalized.to_string()
}

fn is_path_match(a: &str, b: &str) -> bool {
    #[cfg(windows)]
    return a.eq_ignore_ascii_case(b);
    #[cfg(not(windows))]
    return a == b;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_get_system_info() {
        let info = get_sys_info().await.unwrap();
        assert!(!info.os.name.is_empty());
        assert!(!info.os.arch.is_empty());
        assert!(!info.cpu.name.is_empty());
        assert!(info.cpu.count > 0);
        assert!(info.mem.total > 0);
        assert!(!info.drive.drive_format.is_empty());
        assert!(info.drive.total > 0);
    }

    #[test]
    fn test_get_os_info() {
        let os = get_os_info();
        assert!(!os.name.is_empty());
        assert!(!os.arch.is_empty());
    }

    #[tokio::test]
    async fn test_get_cpu_info() {
        let cpu = get_cpu_info().await.unwrap();
        assert!(!cpu.vendor.is_empty());
        assert!(!cpu.name.is_empty());
        assert!(cpu.count > 0);
        assert!(cpu.usage >= 0.0);
    }

    #[test]
    fn test_get_mem_info() {
        let mem = get_mem_info();
        assert!(mem.total > 0);
        assert!(mem.free <= mem.total);
    }

    #[test]
    fn test_get_disk_info() {
        let drive = get_disk_info().unwrap();
        assert!(!drive.drive_format.is_empty());
        assert!(drive.total > 0);
        assert!(drive.free <= drive.total);
    }
}
