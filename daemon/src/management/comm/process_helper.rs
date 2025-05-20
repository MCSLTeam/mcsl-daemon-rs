use mcsl_protocol::management::instance::InstanceProcessMetrics;
#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::Pid;
use std::io;
use sysinfo::{ProcessRefreshKind, System};
use tokio::time::{sleep, Duration};
#[cfg(windows)]
use winapi::shared::minwindef::{DWORD, FALSE};
#[cfg(windows)]
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
#[cfg(windows)]
use winapi::um::tlhelp32::{
    CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
};
#[cfg(windows)]
use winapi::um::winnt::PROCESS_TERMINATE;

pub struct ProcessHelper;

impl ProcessHelper {
    /// Sends SIGTERM to the process with the given ID (Unix) or terminates gracefully (Windows).
    pub fn term(pid: u32) -> io::Result<()> {
        #[cfg(unix)]
        {
            let pid = Pid::from_raw(pid as i32);
            kill(pid, Signal::SIGTERM).map_err(io::Error::other)?;
            Ok(())
        }
        #[cfg(windows)]
        {
            // On Windows, attempt to open the process and terminate it gracefully
            let handle = unsafe { OpenProcess(PROCESS_TERMINATE, FALSE, pid) };
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let result = unsafe { TerminateProcess(handle, 1) };
            unsafe { CloseHandle(handle) };
            if result == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    /// Retrieves child process IDs for a given parent process ID.
    /// On Windows, filters by command line if cmdline_contains is provided.
    #[cfg(windows)]
    pub fn child_id(parent_pid: u32, cmdline_contains: Option<&str>) -> io::Result<Vec<u32>> {
        let mut result = Vec::new();
        let snapshot = unsafe {
            CreateToolhelp32Snapshot(0x00000002 /* TH32CS_SNAPPROCESS */, 0)
        };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let mut entry: PROCESSENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as DWORD;

        if unsafe { Process32First(snapshot, &mut entry) } != 0 {
            loop {
                debug!("traverse pid: {}", entry.th32ProcessID);
                if entry.th32ParentProcessID == parent_pid {
                    // Check command line if provided
                    let matches_cmdline = if let Some(search) = cmdline_contains {
                        let cmdline = unsafe {
                            std::ffi::CStr::from_ptr(entry.szExeFile.as_ptr())
                                .to_string_lossy()
                                .into_owned()
                        };
                        cmdline.contains(search)
                    } else {
                        true
                    };

                    if matches_cmdline {
                        result.push(entry.th32ProcessID);
                    }
                }
                if unsafe { Process32Next(snapshot, &mut entry) } == 0 {
                    break;
                }
            }
        }

        unsafe { CloseHandle(snapshot) };
        Ok(result)
    }

    /// 在容器环境中需要适当权限。
    pub async fn get_process_metrics(pid: u32) -> anyhow::Result<InstanceProcessMetrics> {
        let mut system = System::new_all();

        // 将 u32 PID 转换为 sysinfo 的 Pid 类型
        let pid = sysinfo::Pid::from_u32(pid);

        // 刷新系统信息，包括进程数据
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );

        // 检查进程是否存在
        system
            .process(pid)
            .ok_or_else(|| anyhow::anyhow!("Process(pid={}) not existed", pid))?;

        // 异步等待一段时间以测量 CPU 使用率
        sleep(Duration::from_millis(500)).await;

        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );

        // 获取更新后的进程数据
        let process = system
            .process(pid)
            .ok_or_else(|| anyhow::anyhow!("Process(pid={}) exited after refresh", pid))?;

        // 计算 CPU 使用率（百分比）
        // sysinfo 返回的 CPU 使用率是基于单核的，需除以 CPU 核心数
        let cpu_count = system.cpus().len() as f64;
        let cpu_usage = process.cpu_usage() as f64 / cpu_count;

        // 内存占用已为字节单位
        let memory_usage = process.memory();

        Ok(InstanceProcessMetrics {
            cpu: cpu_usage,
            memory: memory_usage,
        })
    }
}

// Example usage
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn test_process_helper() {
        // This is just a placeholder for testing
        // Actual testing would require running processes and platform-specific handling
        let result = ProcessHelper::child_id(1, None);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_process_metrics() {
        // 使用当前进程的 PID 进行测试
        let pid = std::process::id();

        match ProcessHelper::get_process_metrics(pid).await {
            Ok(metrics) => {
                println!("CPU usage: {:.2}%", metrics.cpu);
                println!("Memory usage: {} B", metrics.memory);
                assert!(metrics.cpu >= 0.0);
                assert!(metrics.memory > 0);
            }
            Err(e) => panic!("Failed to get process metric: {:?}", e),
        }
    }
}
