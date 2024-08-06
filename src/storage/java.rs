use std::cell::{LazyCell, RefCell};
use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::iter::{IntoIterator, Iterator};
use std::path::{absolute, Path, PathBuf};
use std::process::{Output, Stdio};
use std::rc::Rc;
use std::string::ToString;
use std::sync::LazyLock;

use anyhow::{anyhow, bail, Context};
use log::{debug, info, trace, warn};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

#[allow(clippy::declare_interior_mutable_const)]
static MATCHED_KEYS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let keys = [
        "intellij",
        "cache",
        "官启",
        "vape",
        "组件",
        "我的",
        "liteloader",
        "运行",
        "pcl",
        "bin",
        "appcode",
        "untitled folder",
        "content",
        "microsoft",
        "program",
        "lunar",
        "goland",
        "download",
        "corretto",
        "dragonwell",
        "客户",
        "client",
        "新建文件夹",
        "badlion",
        "usr",
        "temp",
        "ext",
        "run",
        "server",
        "软件",
        "software",
        "arctime",
        "jdk",
        "phpstorm",
        "eclipse",
        "rider",
        "x64",
        "jbr",
        "环境",
        "jre",
        "env",
        "jvm",
        "启动",
        "未命名文件夹",
        "sigma",
        "mojang",
        "daemon",
        "craft",
        "oracle",
        "vanilla",
        "lib",
        "file",
        "msl",
        "x86",
        "bakaxl",
        "高清",
        "local",
        "mod",
        "原版",
        "webstorm",
        "应用",
        "hotspot",
        "fabric",
        "整合",
        "net",
        "mine",
        "服务",
        "opt",
        "home",
        "idea",
        "clion",
        "path",
        "android",
        "green",
        "zulu",
        "官方",
        "forge",
        "游戏",
        "blc",
        "user",
        "国服",
        "pycharm",
        "3dmark",
        "data",
        "roaming",
        "程序",
        "java",
        "前置",
        "soar",
        "1.",
        "mc",
        "世界",
        "jetbrains",
        "cheatbreaker",
        "game",
        "网易",
        "launch",
        "fsm",
        "root",
    ];

    let mut new_keys = Vec::with_capacity(keys.len() + 1);
    keys.into_iter().for_each(|k| {
        new_keys.push(k.to_string());
    });

    let output = std::process::Command::new("whoami")
        .output()
        .unwrap()
        .stdout;
    let user = String::from_utf8_lossy(&output)
        .trim()
        .split("\\")
        .map(String::from)
        .collect::<Vec<_>>()
        .last()
        .map(String::from);

    if let Some(user) = user {
        new_keys.push(user);
    }

    new_keys
});


const EXCLUDED_KEYS: [&str; 5] = ["$", "{", "}", "__", "office"];

fn check_java_version(version_str: &str) -> anyhow::Result<()> {
    // (\d+)(?:\.(\d+))?(?:\.(\d+))?(?:[._](\d+))?(?:-(.+))?

    let mut parts = version_str.splitn(5, |c| c == '.' || c == '_' || c == '-');
    parts.next().unwrap_or("").parse::<u32>()?; // major version (required)
    parts.next().unwrap_or("0").parse::<u32>()?; // minor version
    parts.next().unwrap_or("0").parse::<u32>()?; // patch version

    let build = parts.next();
    if build.is_none() {
        return Ok(());
    }

    if build.is_some_and(|s| s.chars().all(|c| c.is_ascii_digit())) {
        Ok(())
    } else {
        bail!("Invalid java version")
    }

    // suffix we don't care
}

fn scan<P: AsRef<Path>>(
    path: P,
    pending_map: &mut HashMap<String, JoinHandle<anyhow::Result<JavaInfo>>>,
    filename: &'static str,
    recursive: bool,
) -> () {
    if path.as_ref().is_file() {
        return;
    }

    let dir = match path.as_ref().read_dir() {
        Ok(dir) => dir,
        Err(_) => return,
    };

    for entry in dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => return,
        };
        let path = entry.path();
        let abs_path = absolute(path.as_path()).unwrap();
        let abs_path_str = abs_path.to_string_lossy().to_string();
        let name = path.file_name().unwrap().to_str().unwrap();
        if path.is_file() {
            if path.file_name().unwrap().to_str().unwrap() == filename {
                debug!("Found java: {}", abs_path.display());

                // if pending_map.contains_key(&abs_path_str) {
                //     info!("ignore java: {}", &abs_path_str);
                //     continue;
                // }

                // async get java info
                #[cfg(windows)]
                let child = Command::new(abs_path.as_os_str())
                    .arg("-version")
                    .creation_flags(0x08000000) // refer to https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
                    .output();

                #[cfg(not(windows))]
                let child = Command::new(abs_path.as_os_str()).arg("-version").output();

                let abs_path_str_clone = abs_path_str.clone();
                let handler =
                    tokio::spawn(async move { mapper(abs_path_str_clone, child.await?).await });

                pending_map.insert(abs_path_str, handler);
            }
        } else if (*EXCLUDED_KEYS)
            .iter()
            .any(|k| name.to_lowercase().contains(k))
        {
            continue;
        } else if recursive
            && (*MATCHED_KEYS)
                .iter()
                .any(|k| name.to_ascii_lowercase().contains(k))
        {
            scan(path, pending_map, filename, recursive) // recursive
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct JavaInfo {
    pub version: String,
    pub path: String,
    pub arch: String,
}

async fn mapper(path: String, mut output: Output) -> anyhow::Result<JavaInfo> {
    if output.status.success() {
        let out = String::from_utf8_lossy(&output.stderr).to_string();

        let mut version = out
            .split("\"")
            .nth(1)
            .ok_or(anyhow!("Failed to get java version"))?
            .to_string();

        if check_java_version(&version).is_err(){
            version = "Unknown".to_string();
        }

        let arch = if out.contains("64-Bit") { "x64" } else { "x86" }.to_string();

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
    let mut handle_map = HashMap::new();

    let java_filename = if cfg!(windows) { "java.exe" } else { "java" };

    // scan disk
    #[cfg(windows)]
    {
        for disk in "CDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
            let disk_path = format!("{}:\\", disk);
            let path = Path::new(&disk_path);
            if path.exists() {
                scan(path, &mut handle_map, java_filename, true);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let path = Path::new("/");
        scan(path, &handle_map, java_filename, true, &filter);
    }
    trace!("start scan PATH");

    // scan PATH
    if let Some(paths) = env::var_os("PATH") {
        for path in env::split_paths(&paths) {
            let path_str = path.to_string_lossy().to_string();

            if handle_map.keys().any(|k| k.starts_with(&path_str)) {
                trace!("ignore path: {}", path_str);
                continue;
            }

            trace!("scan path: {}", path_str);
            scan(path, &mut handle_map, java_filename, false)
        }
    }

    let mut rv = vec![];

    for (_, handle) in handle_map.drain() {
        if let Ok(info) = handle.await {
            match info {
                Ok(info) => rv.push(info),
                Err(ref err) => {
                    warn!("{:?}", err)
                }
            }
        }
    }
    Ok(rv)
}
