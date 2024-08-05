use crate::app::run_app;

mod user;
mod storage;
mod utils;
mod app;
mod remote;

fn init_logger(){
    std::env::set_var("RUST_LOG", "trace");
    pretty_env_logger::init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()>{
    init_logger();
    run_app().await
}
