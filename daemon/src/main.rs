use crate::app::run_app;

mod app;
mod auth;
pub mod config;
mod drivers;
mod management;
mod protocols;
mod storage;
mod utils;

fn init_logger() {
    unsafe {
        std::env::set_var("RUST_LOG", "debug");
    }
    pretty_env_logger::init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logger();
    run_app().await
}
