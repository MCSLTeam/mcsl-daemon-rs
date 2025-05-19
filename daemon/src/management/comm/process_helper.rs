use std::io;
use std::process::{Child, Command, ExitStatus};
use log::debug;
#[cfg(windows)]
use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
#[cfg(windows)]
use winapi::um::tlhelp32::{CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32};
#[cfg(windows)]
use winapi::um::winnt::PROCESS_TERMINATE;
#[cfg(windows)]
use winapi::shared::minwindef::{FALSE, DWORD};
#[cfg(windows)]
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

pub struct ProcessHelper;

impl ProcessHelper {
    /// Sends SIGTERM to the process with the given ID (Unix) or terminates gracefully (Windows).
    pub fn stop(pid: u32) -> io::Result<()> {
        #[cfg(unix)]
        {
            let pid = Pid::from_raw(pid as i32);
            kill(pid, Signal::SIGTERM)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            Ok(())
        }
        #[cfg(windows)]
        {
            // On Windows, attempt to open the process and terminate it gracefully
            let handle = unsafe {
                OpenProcess(PROCESS_TERMINATE, FALSE, pid)
            };
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let result = unsafe {
                TerminateProcess(handle, 1)
            };
            unsafe { CloseHandle(handle) };
            if result == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    /// Forcefully kills the process. On Windows, uses WinAPI TerminateProcess.
    /// On Unix, sends SIGKILL.
    pub fn kill(pid: u32) -> io::Result<()> {
        #[cfg(unix)]
        {
            let pid = Pid::from_raw(pid as i32);
            kill(pid, Signal::SIGKILL)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            Ok(())
        }
        #[cfg(windows)]
        {
            let handle = unsafe {
                OpenProcess(PROCESS_TERMINATE, FALSE, pid)
            };
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let result = unsafe {
                TerminateProcess(handle, 1)
            };
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

    /// Retrieves child process IDs for a given parent process ID (Unix).
    /// cmdline_contains is ignored on Unix as it's Windows-specific.
    #[cfg(unix)]
    pub fn child_id(parent_pid: u32, _cmdline_contains: Option<&str>) -> io::Result<Vec<u32>> {
        use std::fs;
        let mut result = Vec::new();
        let paths = fs::read_dir("/proc")?;

        for entry in paths {
            let entry = entry?;
            if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() {
                if let Ok(stat) = fs::read_to_string(format!("/proc/{}/stat", pid)) {
                    let parts: Vec<&str> = stat.split_whitespace().collect();
                    if parts.len() > 3 {
                        if let Ok(ppid) = parts[3].parse::<u32>() {
                            if ppid == parent_pid {
                                result.push(pid);
                            }
                        }
                    }
                }
            }
        }
        Ok(result)
    }
}

// Example usage
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_helper() {
        // This is just a placeholder for testing
        // Actual testing would require running processes and platform-specific handling
        let result = ProcessHelper::child_id(1, None);
        assert!(result.is_ok());
    }
}