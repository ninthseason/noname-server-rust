use fastwebsockets::WebSocketError;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use resource_manager::ResourceManager;
use tokio::net::TcpListener;
use types::*;
mod logic;
mod message;
mod resource_manager;
mod session;
mod types;
mod util;

#[tokio::main]
async fn main() -> Result<(), WebSocketError> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;

    let mut resource_manager = ResourceManager::new();
    let (rmc_tx, rmc_rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move { resource_manager.start(rmc_rx).await });

    println!("Server started, listening on {}", "0.0.0.0:8080");
    loop {
        let (stream, addr) = listener.accept().await?;
        let rmc_tx = rmc_tx.clone();
        println!("[+]Client connected from {}:{}", addr.ip(), addr.port());
        tokio::spawn(async move {
            let io = hyper_util::rt::TokioIo::new(stream);
            let conn_fut = http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(|req| logic::server_upgrade(req, rmc_tx.clone())),
                )
                .with_upgrades();
            if let Err(e) = conn_fut.await {
                println!("An error occurred: {:?}", e);
            }
        });
    }
}
