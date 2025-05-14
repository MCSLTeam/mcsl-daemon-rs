use crate::app::AppState;
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};
use std::net::SocketAddr;

type Body = http_body_util::Full<Bytes>;
pub async fn verify_connection(
    app_state: AppState,
    req: &Request<Incoming>,
    remote_addr: &SocketAddr,
) -> Result<(), Response<Body>> {
    Ok(())
}
