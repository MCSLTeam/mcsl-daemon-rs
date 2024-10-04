use std::collections::HashMap;
use std::env;
use std::iter::Iterator;
use std::path::{absolute, Path};
use std::process::Output;
use std::string::ToString;

use anyhow::{anyhow, bail};
use log::{debug, trace, warn};
use tokio::process::Command;
use tokio::task::JoinHandle;

const MATCH_KEYS: [&str; 101] = [
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
    "bellsoft",
    "libericajdk",
];

const EXCLUDED_KEYS: [&str; 5] = ["$", "{", "}", "__", "office"];

static mut USER_NAME: Option<String> = None;

fn get_user_name() -> String {
    unsafe {
        // &: Option<T> -> &Option<T>
        // .as_ref(): Option<T> -> Option<&T>
        if let Some(user) = USER_NAME.as_ref() {
            return user.to_owned();
        }

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
            .map(String::from)
            .unwrap();

        USER_NAME = Some(user.clone());
        user
    }
}

fn verify_java_version(version_str: &str) -> anyhow::Result<()> {
    // (\d+)(?:\.(\d+))?(?:\.(\d+))?(?:[._](\d+))?(?:-(.+))?

    let mut parts = version_str.splitn(5, ['.', '_', '-']);
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

pub const JAVA_NAME: &str = "java";

fn scan<P>(
    path: P,
    pending_map: &mut HashMap<String, JoinHandle<anyhow::Result<JavaInfo>>>,
    recursive: bool,
) where
    P: AsRef<Path>,
{
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
        if pending_map.contains_key(&abs_path_str) {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if path.is_file() {
            let file_match = path
                .file_stem()
                .unwrap() // unwrap safe: 你搜索的时候又不会搜到 .. 结尾或者 .. 中间的文件名
                .to_str()
                .map(|name| name.to_ascii_lowercase() == JAVA_NAME);
            #[cfg(windows)]
            {
                // 额外匹配 .exe 的后缀
                file_match = file_match.map(|origin| {
                    origin && path.extension().map(|ext| ext == "exe").unwrap_or(false)
                });
            }
            if file_match.unwrap_or(false) {
                debug!("Found java: {}", abs_path.display());

                // async get java info
                let mut runner = Command::new(abs_path.as_os_str());
                runner.arg("-version");
                #[cfg(windows)]
                {
                    runner.creation_flags(0x08000000);
                    // refer to https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
                }
                let child = runner.output();

                let abs_path_str_ = abs_path_str.clone();
                let handler = tokio::spawn(async move {
                    JavaInfo::try_from_path_output(abs_path_str_, child.await?)
                });

                pending_map.insert(abs_path_str, handler);
            }
        } else if EXCLUDED_KEYS
            .iter()
            .any(|k| name.to_lowercase().contains(k))
        {
            continue;
        } else if recursive
            && (MATCH_KEYS
                .iter()
                .any(|k| name.to_ascii_lowercase().contains(k))
                || name == get_user_name())
        {
            scan(path, pending_map, recursive)
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct JavaInfo {
    pub version: String,
    pub path: String,
    pub arch: String,
}

impl JavaInfo {
    fn try_from_path_output(path: String, output: Output) -> anyhow::Result<JavaInfo> {
        if output.status.success() {
            let out = String::from_utf8_lossy(&output.stderr).to_string();

            let mut version = out
                .split("\"")
                .nth(1)
                .ok_or(anyhow!("Failed to get java version"))?
                .to_string();

            if verify_java_version(&version).is_err() {
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
}

pub async fn java_scan() -> Vec<JavaInfo> {
    let mut handle_map = HashMap::new();

    trace!("start scan PATH");

    // scan PATH
    if let Some(paths) = env::var_os("PATH") {
        for path in env::split_paths(&paths) {
            let path_str = path.to_string_lossy().to_string();

            trace!("scan path: {}", path_str);
            scan(path, &mut handle_map, false)
        }
    }
    // scan disk
    #[cfg(windows)]
    {
        for disk in "CDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
            let disk_path = format!("{}:\\", disk);
            let path = Path::new(&disk_path);
            if path.exists() {
                scan(path, &handle_map, true);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let path = Path::new("/");
        scan(path, &mut handle_map, true);
    }

    let mut rv = vec![];

    for (_, handle) in handle_map.into_iter() {
        if let Ok(info) = handle.await {
            match info {
                Ok(info) => rv.push(info),
                Err(ref err) => {
                    warn!("{:?}", err)
                }
            }
        }
    }
    rv
}
