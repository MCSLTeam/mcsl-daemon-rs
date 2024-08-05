use anyhow::{anyhow, bail, Context};
use log::{debug, trace, warn};
use std::cell::LazyCell;
use std::ffi::OsStr;
use std::iter::{IntoIterator, Iterator};
use std::os::windows::process::CommandExt;
use std::path::{absolute, Path, PathBuf};
use std::process::Stdio;
use std::string::ToString;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

#[allow(clippy::declare_interior_mutable_const)]
const MATCHED_KEYS: LazyCell<Vec<String>> = LazyCell::new(|| {
    let mut keys: Vec<_> = vec![
        "1.",
        "bin",
        "cache",
        "client",
        "craft",
        "data",
        "download",
        "eclipse",
        "mine",
        "mc",
        "launch",
        "hotspot",
        "java",
        "jdk",
        "jre",
        "zulu",
        "dragonwell",
        "jvm",
        "microsoft",
        "corretto",
        "sigma",
        "mod",
        "mojang",
        "net",
        "netease",
        "forge",
        "liteloader",
        "fabric",
        "game",
        "vanilla",
        "server",
        "opt",
        "oracle",
        "path",
        "program",
        "roaming",
        "local",
        "run",
        "runtime",
        "software",
        "daemon",
        "temp",
        "users",
        "users",
        "x64",
        "x86",
        "lib",
        "usr",
        "env",
        "ext",
        "file",
        "data",
        "green",
        "我的",
        "世界",
        "前置",
        "原版",
        "启动",
        "启动",
        "国服",
        "官启",
        "官方",
        "客户",
        "应用",
        "整合",
        "组件",
        "新建文件夹",
        "服务",
        "游戏",
        "环境",
        "程序",
        "网易",
        "软件",
        "运行",
        "高清",
        "badlion",
        "blc",
        "lunar",
        "tlauncher",
        "soar",
        "cheatbreaker",
        "hmcl",
        "pcl",
        "bakaxl",
        "fsm",
        "vape",
        "jetbrains",
        "intellij",
        "idea",
        "pycharm",
        "webstorm",
        "clion",
        "goland",
        "rider",
        "datagrip",
        "rider",
        "appcode",
        "phpstorm",
        "rubymine",
        "jbr",
        "android",
        "mcsm",
        "msl",
        "mcsl",
        "3dmark",
        "arctime",
        "library",
        "content",
        "home",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let output = std::process::Command::new("whoami")
        .output()
        .unwrap()
        .stdout;
    let users = String::from_utf8_lossy(&output)
        .split("\\")
        .map(String::from)
        .collect::<Vec<_>>().last().unwrap().to_string();
    keys.push(users);
    keys
});

const EXCLUDED_KEYS: LazyCell<Vec<String>> = LazyCell::new(|| {
    ["$", "{", "}", "__", "office"]
        .into_iter()
        .map(String::from)
        .collect()
});

fn scan<P: AsRef<Path>>(
    path: P,
    pendings: &mut Vec<JoinHandle<anyhow::Result<JavaInfo>>>,
    filename: &'static str,
) -> () {
    if path.as_ref().is_file() {
        return;
    }

    let dir = match path.as_ref().read_dir() {
        Ok(dir) => dir,
        Err(e) => {
            warn!("Failed to read dir: {}", e);
            return;
        }
    };

    for entry in dir {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                warn!("Failed to read dir: {}", err);
                return;
            }
        };
        let path = entry.path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if path.is_file() {
            if path.file_name().unwrap().to_str().unwrap() == filename {
                let abs_path = absolute(path.as_path()).unwrap();
                debug!("Found java: {}", abs_path.display());

                // async get java info
                #[cfg(windows)]
                let child = Command::new(abs_path.as_os_str())
                    .arg("-version")
                    .creation_flags(0x08000000) // refer to https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
                    .spawn();

                #[cfg(not(windows))]
                let child = Command::new(abs_path.as_os_str())
                    .arg("-version")
                    .spawn();

                match child {
                    Ok(child) => {
                        let handler = tokio::spawn(async move {
                            mapper(abs_path.to_str().unwrap().to_string(), child).await
                        });

                        pendings.push(handler);
                    }
                    Err(err) => {
                        warn!("Failed to spawn java process: {}", err);
                        continue;
                    }
                }
            }
        } else if (*EXCLUDED_KEYS)
            .iter()
            .any(|k| name.to_lowercase().contains(k))
        {
            continue;
        } else if (*MATCHED_KEYS)
            .iter()
            .any(|k| name.to_ascii_lowercase().contains(k))
        {
            scan(path, pendings, filename) // recursive
        }
    }
}

#[derive(Debug, Clone)]
pub struct JavaInfo {
    pub version: String,
    pub path: String,
    pub arch: String,
}

async fn mapper(path: String, mut child: Child) -> anyhow::Result<JavaInfo> {
    let output = child.wait().await?;
    if output.success() {
        let mut stdout = String::new();
        child.stdout.take().ok_or(anyhow!("Failed to read stderr"))?.read_to_string(&mut stdout).await?;

        let mut version = stdout
            .split("\"")
            .nth(1)
            .ok_or(anyhow!("Failed to get java version"))?
            .to_string();

        // check version
        if version.chars().any(|c| c != '.' && !c.is_ascii_digit()) {
            version = "Unknown".to_string();
        }

        let arch = if stdout.contains("64-Bit") {
            "x64"
        } else {
            "x86"
        }
        .to_string();

        Ok(JavaInfo {
            version,
            path,
            arch,
        })
    } else {
        Err(anyhow!("Failed to get java version"))
    }
}

pub async fn java_scan() -> anyhow::Result<Vec<JavaInfo>> {
    let mut handlers = vec![];
    #[cfg(windows)]
    {
        for disk in "CDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
            let disk_path = format!("{}:\\", disk);
            let path = Path::new(&disk_path);
            if path.exists() {
                scan(path, &mut handlers, "java.exe");
            }
        }
    }

    #[cfg(not(windows))]
    {
        let path = Path::new("/");
        scan(path, &mut handlers, "java")?;
    }

    let mut rv = vec![];

    for handler in handlers {
        if let Ok(res) = handler.await {
            match res {
                Ok(info) => rv.push(info),
                Err(err) => warn!("Failed to get java version: {}", err),
            }
        }
    }
    Ok(rv)
}
