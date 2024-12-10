use crate::app::run_app;

mod app;
mod drivers;
mod minecraft;
mod protocols;
mod storage;
mod user;
mod utils;

fn init_logger() {
    unsafe {
        std::env::set_var("RUST_LOG", "trace");
    }
    pretty_env_logger::init();
}

// async fn scan_java() -> anyhow::Result<()> {
//     let begin = std::time::Instant::now();
//     let rv = storage::java::java_scan().await;
//     for item in rv {
//         info!("{} {} {}", item.version, item.path, item.arch);
//     }
//     info!("java search elapsed: {}ms", begin.elapsed().as_millis());
//     Ok(())
// }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logger();
    run_app().await
}
