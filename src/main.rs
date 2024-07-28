use env_logger::Env;

mod user;
mod storage;
mod utils;
mod base64;
mod app;




fn init_logger(){
    let env = Env::default()
        .filter_or("MY_LOG_LEVEL", "trace")
        .write_style_or("MY_LOG_STYLE", "always");

    env_logger::init_from_env(env);
}

#[tokio::main]
async fn main() -> anyhow::Result<()>{
    init_logger();

    let app = app::App::new();
    app.start().await?;
    Ok(())
}