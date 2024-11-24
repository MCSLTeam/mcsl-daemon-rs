use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::iter::Iterator;
use std::path::{absolute, Path};
use std::process::Output;
use std::string::ToString;
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

use anyhow::anyhow;
use log::{debug, trace, warn};
use tokio::process::Command;
use tokio::task::{JoinHandle, JoinSet};

use crate::utils::AsyncFetchable;

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

static USER_NAME: LazyLock<String> = LazyLock::new(get_user_name);
static JAVA_VERSION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:[._](\d+))?(?:-(.+))?").unwrap());

type JoinHandleMap<K, V> = Arc<Mutex<HashMap<K, JoinHandle<anyhow::Result<V>>>>>;

fn get_user_name() -> String {
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
    user
}

pub const JAVA_NAME: &str = "java";

fn scan<P>(path: P, join_handle_map: JoinHandleMap<String, JavaInfo>, recursive: bool)
where
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
        let name = path.file_name().and_then(OsStr::to_str).unwrap();
        if path.is_file() {
            let file_match = path
                .file_stem()
                .unwrap() // unwrap safe: 你搜索的时候又不会搜到 .. 结尾或者 .. 中间的文件名
                .to_str()
                .map(|name| {
                    let name_lower = name.to_ascii_lowercase();
                    if cfg!(windows) {
                        name_lower == JAVA_NAME
                            && path.extension().map_or(false, |ext| ext == "exe")
                    } else {
                        name_lower == JAVA_NAME
                    }
                })
                .unwrap_or(false);
            if file_match {
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

                let mut map_guard = futures::executor::block_on(join_handle_map.lock());
                map_guard.entry(abs_path_str).or_insert(handler);
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
                || name == *USER_NAME)
        {
            let join_handle_map = join_handle_map.clone();
            scan(path, join_handle_map, recursive)
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct JavaInfo {
    pub version: String,
    pub path: String,
    pub arch: String,
}

impl JavaInfo {
    fn try_from_path_output(path: String, output: Output) -> anyhow::Result<JavaInfo> {
        if output.status.success() {
            let out = String::from_utf8_lossy(&output.stderr).to_string();

            let version = JAVA_VERSION_REGEX
                .find(&out)
                .map(|m| m.as_str())
                .unwrap_or("Unknown")
                .to_string();

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
    let join_handle_map = Arc::new(Mutex::new(HashMap::new()));

    trace!("start scan PATH");

    let mut task_set = JoinSet::new();
    // scan PATH
    if let Some(paths) = env::var_os("PATH") {
        for path in env::split_paths(&paths) {
            let path_str = path.to_string_lossy().to_string();

            trace!("scan path: {}", path_str);
            let join_handle_map = join_handle_map.clone();

            // add scan task
            task_set.spawn_blocking(move || scan(path, join_handle_map, true));
        }
    }
    // scan disk
    #[cfg(windows)]
    {
        for disk in "CDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
            let disk_path = format!("{}:\\", disk);
            if fs::metadata(&disk_path).is_ok() {
                let join_handle_map = join_handle_map.clone();
                // add scan task
                task_set.spawn_blocking(move || {
                    let path = Path::new(&disk_path);
                    scan(path, join_handle_map, true)
                });
            }
        }
    }
    #[cfg(not(windows))]
    {
        let path = Path::new("/");
        let join_handle_map = join_handle_map.clone();
        // add scan task
        task_set.spawn_blocking(move || scan(path, join_handle_map, true));
    }

    // wait all scan tasks and then wait all join handles for result
    while task_set.join_next().await.is_some() {}

    let mut rv = vec![];
    let mut map_guard = join_handle_map.lock().await;
    for (_, handle) in map_guard.drain() {
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

impl AsyncFetchable for Vec<JavaInfo> {
    async fn fetch() -> Self {
        java_scan().await
    }
}
