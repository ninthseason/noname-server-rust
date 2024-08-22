use fastwebsockets::upgrade;
use fastwebsockets::FragmentCollectorRead;
use fastwebsockets::WebSocketError;
use http_body_util::Empty;
use hyper::body::Bytes;
use hyper::body::Incoming;
use hyper::Request;
use hyper::Response;
use tokio::sync::mpsc;

use crate::resource_manager::RMCMessage;
use crate::session::Session;
use crate::SessionItem;

async fn handle_client(
    fut: upgrade::UpgradeFut,
    rmc_tx: mpsc::Sender<RMCMessage>,
) -> Result<(), WebSocketError> {
    let ws = fut.await?;
    let (rx, tx) = ws.split(tokio::io::split);
    let rx = FragmentCollectorRead::new(rx);
    let mut session = Session::new(tx, rmc_tx.clone());
    rmc_tx
        .send(RMCMessage::InsertSession(
            session.get_wsid(),
            SessionItem {
                controller: session.get_controller(),
                state: session.get_state(),
            },
        ))
        .await
        .unwrap();
    session.start(rx).await;
    println!("[-]Session {} leaved.", session.get_wsid());
    Ok(())
}

pub async fn server_upgrade(
    mut req: Request<Incoming>,
    rmc_tx: mpsc::Sender<RMCMessage>,
) -> Result<Response<Empty<Bytes>>, WebSocketError> {
    let (response, fut) = upgrade::upgrade(&mut req)?;

    tokio::task::spawn(async move {
        if let Err(e) = tokio::task::unconstrained(handle_client(fut, rmc_tx)).await {
            eprintln!("Error in websocket connection: {}", e);
        }
    });

    Ok(response)
}
