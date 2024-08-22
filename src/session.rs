use fastwebsockets::{Frame, OpCode, WebSocketError};
use tokio::sync::{mpsc, oneshot};

use crate::{
    message::{ControlMessage, RecvMessage, SendMessage},
    resource_manager::RMCMessage,
    types::*,
    util,
};

pub struct Session {
    wsid: String,
    wstx: WebsocketSender,
    control_tx: ControlChannelSender, // 单独控制通道
    control_rx: ControlChannelReceiver,
    rmc_tx: mpsc::Sender<RMCMessage>,
    state: SessionState,
}

impl Session {
    pub fn new(wstx: WebsocketSender, rmc_tx: mpsc::Sender<RMCMessage>) -> Session {
        let (control_tx, control_rx) = tokio::sync::mpsc::channel::<ControlMessage>(32);
        let wsid = util::generate_wsid();
        Session {
            state: SessionState::new(wsid.clone()),
            wsid,
            wstx,
            control_tx,
            control_rx,
            rmc_tx,
        }
    }

    pub fn get_state(&self) -> SessionState {
        self.state.clone()
    }
    pub fn get_controller(&self) -> ControlChannelSender {
        self.control_tx.clone()
    }
    pub fn get_wsid(&self) -> String {
        self.wsid.clone()
    }

    pub async fn start(&mut self, mut wsrx: WebsocketReader) {
        let control_tx = self.control_tx.clone();
        // 心跳
        tokio::spawn(async move {
            loop {
                let rc = control_tx.send(ControlMessage::Heartbeat).await;
                if rc.is_err() {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            }
        });
        self.send(SendMessage::RoomList).await;
        let mut last_beat = true;
        loop {
            tokio::select! {
                msg = Session::read(&mut wsrx) => {
                    match msg {
                        Ok(msg) => {
                            if let RecvMessage::Heartbeat = msg {
                                last_beat = true;
                            } else {
                                self.dispatch(msg).await;
                            }
                        }
                        Err(e) => {
                            println!("[!]session {} closed with {}", self.wsid, e);
                            break;
                        }
                    }
                },
                Some(msg) = self.control_rx.recv() => {
                    match msg {
                        ControlMessage::Send(msg) => {
                            self.send(msg).await;
                        }
                        ControlMessage::Heartbeat => {
                            if last_beat {
                                last_beat = false;
                                self.send(SendMessage::Heartbeat).await;
                            } else {
                                println!("[!]session {} heartbeat failed", self.wsid);
                                break;
                            }
                        }
                        ControlMessage::Close => {
                            break;
                        }
                    }
                }
            }
        }
        self.close().await;
    }

    pub async fn send(&mut self, msg: SendMessage) {
        match msg {
            SendMessage::RoomList => {
                let (tx, rx) = oneshot::channel();
                let (tx2, rx2) = oneshot::channel();
                self.rmc_tx.send(RMCMessage::GetRoomList(tx)).await.unwrap();
                self.rmc_tx
                    .send(RMCMessage::GetSessionList(tx2))
                    .await
                    .unwrap();
                Session::send_(
                    &mut self.wstx,
                    &format!(
                        r#"["roomlist",{},[],{},"{}"]"#,
                        rx.await.unwrap(),
                        rx2.await.unwrap(),
                        self.wsid,
                    )[..],
                )
                .await
                .unwrap()
            }
            SendMessage::UpdateClients => {
                let (tx, rx) = oneshot::channel();
                self.rmc_tx
                    .send(RMCMessage::GetSessionList(tx))
                    .await
                    .unwrap();
                Session::send_(
                    &mut self.wstx,
                    &format!(r#"["updateclients",{}]"#, rx.await.unwrap(),)[..],
                )
                .await
                .unwrap()
            }
            SendMessage::CreateRoom => {
                let (tx, rx) = oneshot::channel();
                self.rmc_tx
                    .send(RMCMessage::GetRoom(self.wsid.clone(), tx))
                    .await
                    .unwrap();

                Session::send_(
                    &mut self.wstx,
                    &format!(r#"["createroom",{}]"#, rx.await.unwrap().unwrap().key,)[..],
                )
                .await
                .unwrap()
            }
            SendMessage::UpdateRooms => {
                let (tx, rx) = oneshot::channel();
                self.rmc_tx.send(RMCMessage::GetRoomList(tx)).await.unwrap();
                let (tx2, rx2) = oneshot::channel();
                self.rmc_tx
                    .send(RMCMessage::GetSessionList(tx2))
                    .await
                    .unwrap();
                Session::send_(
                    &mut self.wstx,
                    &format!(
                        r#"["updaterooms",{},{}]"#,
                        rx.await.unwrap(),
                        rx2.await.unwrap(),
                    )[..],
                )
                .await
                .unwrap()
            }
            SendMessage::EnterRoomFailed => Session::send_(&mut self.wstx, "enterroomfailed")
                .await
                .unwrap(),
            SendMessage::OnConnection(wsid) => Session::send_(
                &mut self.wstx,
                &format!(r#"["onconnection","{}"]"#, wsid)[..],
            )
            .await
            .unwrap(),
            SendMessage::Plain(text) => Session::send_(&mut self.wstx, &text[..]).await.unwrap(),
            SendMessage::OnMessage(wsid, message) => {
                Session::send_(
                    &mut self.wstx,
                    &format!(r#"["onmessage","{}","{}"]"#, wsid, message)[..],
                )
                .await
                .unwrap()
            }
            SendMessage::SelfClose => Session::send_(&mut self.wstx, "[\"selfclose\"]")
                .await
                .unwrap(),
            SendMessage::OnClose(wsid) => {
                Session::send_(&mut self.wstx, &format!(r#"["onclose","{}"]"#, wsid)[..])
                    .await
                    .unwrap()
            }
            SendMessage::Heartbeat => {
                Session::send_(&mut self.wstx, "heartbeat").await.unwrap()
            }
            _ => {
                self.close().await;
                todo!("not implemented: {:?}", msg);
            }
        }
    }

    async fn read(ws: &mut WebsocketReader) -> Result<RecvMessage, WebSocketError> {
        let frame = Session::read_(ws).await?;
        match frame.opcode {
            OpCode::Close => {
                return Ok(RecvMessage::SessionClose);
            }
            OpCode::Text => {
                if let Ok(cmd) = String::from_utf8(frame.payload.to_vec()) {
                    if cmd == "heartbeat" {
                        return Ok(RecvMessage::Heartbeat);
                    }
                    let tokens = util::parse_command(&cmd);
                    if tokens[0] == "server" {
                        println!("-> {:?}", tokens);
                        match &tokens[1][..] {
                            "key" => {
                                return Ok(RecvMessage::Key(util::parse_key_from_token(
                                    &tokens[2],
                                )));
                            }
                            "changeAvatar" => {
                                return Ok(RecvMessage::ChangeAvatar(
                                    util::trim_nickname(&tokens[2]),
                                    tokens[3].clone(),
                                ));
                            }
                            "create" => {
                                let key = tokens[2].clone();
                                let nickname = util::trim_nickname(&tokens[3]);
                                let avatar = tokens[4].clone();
                                return Ok(RecvMessage::Create(key, nickname, avatar));
                            }
                            "config" => {
                                let config = &cmd[r#"["server","config","#.len()..&cmd.len() - 1];
                                return Ok(RecvMessage::Config(config.to_string()));
                            }
                            "enter" => {
                                let key = tokens[2].clone();
                                let nickname = tokens[3].clone();
                                let avatar = tokens[4].clone();
                                return Ok(RecvMessage::Enter(key, nickname, avatar));
                            }
                            "send" => {
                                let id = tokens[2].clone();
                                let message = tokens[3].clone();
                                return Ok(RecvMessage::Send(id, message));
                            }
                            "status" => {
                                return Ok(RecvMessage::Status(tokens[2].clone()));
                            }
                            "close" => {
                                return Ok(RecvMessage::Close(tokens[2].clone()));
                            }
                            _ => {
                                println!("unknown message: {:?}", tokens)
                            }
                        }
                    } else {
                        return Ok(RecvMessage::ForwardToOwner(util::espace_json(&cmd)));
                    }
                }
            }
            _ => {}
        }
        Ok(RecvMessage::PlaceHolder)
    }

    async fn dispatch(&mut self, msg: RecvMessage) {
        match msg {
            RecvMessage::Key(online_key) => {
                self.state.online_key = Some(online_key);
                self.rmc_tx
                    .send(RMCMessage::UpdateSessionState(
                        self.wsid.clone(),
                        self.state.clone(),
                    ))
                    .await
                    .unwrap();
            }
            RecvMessage::ChangeAvatar(name, avatar) => {
                self.state.nickname = Some(name);
                self.state.avatar = Some(avatar);
                self.rmc_tx
                    .send(RMCMessage::UpdateSessionState(
                        self.wsid.clone(),
                        self.state.clone(),
                    ))
                    .await
                    .unwrap();
                self.rmc_tx
                    .send(RMCMessage::BroadcastMessage(SendMessage::UpdateClients))
                    .await
                    .unwrap();
            }
            RecvMessage::SessionClose => {
                self.close().await;
            }
            RecvMessage::Create(key, nickname, avatar) => {
                if key == *self.state.online_key.as_ref().unwrap() {
                    self.state.nickname = Some(nickname);
                    self.state.avatar = Some(avatar);
                    let room = Room::new(self.state.clone(), key.clone());
                    self.state.room_id = Some(key.clone());
                    self.rmc_tx
                        .send(RMCMessage::UpdateSessionState(
                            self.wsid.clone(),
                            self.state.clone(),
                        ))
                        .await
                        .unwrap();
                    self.rmc_tx
                        .send(RMCMessage::InsertRoom(self.wsid.clone(), room))
                        .await
                        .unwrap();
                    self.send(SendMessage::CreateRoom).await;
                }
            }
            RecvMessage::Config(config) => {
                let (tx, rx) = oneshot::channel();
                self.rmc_tx
                    .send(RMCMessage::GetRoom(self.wsid.clone(), tx))
                    .await
                    .unwrap();
                let room = rx.await.unwrap();
                if let Some(mut room) = room {
                    room.config = Some(config);
                    self.rmc_tx
                        .send(RMCMessage::InsertRoom(self.wsid.clone(), room))
                        .await
                        .unwrap();
                    self.rmc_tx
                        .send(RMCMessage::BroadcastMessage(SendMessage::UpdateRooms))
                        .await
                        .unwrap();
                }
            }
            RecvMessage::Enter(key, nickname, avatar) => {
                self.state.nickname = Some(nickname);
                self.state.avatar = Some(avatar);
                let (tx, rx) = oneshot::channel();
                self.rmc_tx
                    .send(RMCMessage::GetRoomByKey(key.clone(), tx))
                    .await
                    .unwrap();
                let room = rx.await.unwrap();

                match room {
                    Some(room) => {
                        self.state.room_id = Some(key.clone());
                        self.state.status = None;
                        if let Some(_config) = room.config.clone() {
                            // todo #52
                            self.rmc_tx
                                .send(RMCMessage::SendSessionMessage(
                                    room.owner.wsid.clone(),
                                    SendMessage::OnConnection(self.wsid.clone()),
                                ))
                                .await
                                .unwrap();
                        } else {
                            self.state.room_id = None;
                            self.send(SendMessage::EnterRoomFailed).await;
                        }
                    }
                    None => {
                        self.send(SendMessage::EnterRoomFailed).await;
                    }
                }
                self.rmc_tx
                    .send(RMCMessage::UpdateSessionState(
                        self.wsid.clone(),
                        self.state.clone(),
                    ))
                    .await
                    .unwrap();
                self.rmc_tx
                    .send(RMCMessage::BroadcastMessage(SendMessage::UpdateRooms))
                    .await
                    .unwrap();
            }
            RecvMessage::Send(id, message) => {
                let (tx, rx) = oneshot::channel();
                self.rmc_tx
                    .send(RMCMessage::GetSessionState(id, tx))
                    .await
                    .unwrap();
                if let Some(state) = rx.await.unwrap() {
                    if let Some(room_id) = state.room_id {
                        let (tx, rx) = oneshot::channel();
                        self.rmc_tx
                            .send(RMCMessage::GetRoomByKey(room_id.clone(), tx))
                            .await
                            .unwrap();
                        let room = rx.await.unwrap();
                        if let Some(room) = room {
                            if room.owner.wsid == self.wsid {
                                self.rmc_tx
                                    .send(RMCMessage::SendSessionMessage(
                                        state.wsid.clone(),
                                        SendMessage::Plain(message),
                                    ))
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                }
            }
            RecvMessage::ForwardToOwner(message) => {
                if let Some(room_id) = self.state.room_id.clone() {
                    let (tx, rx) = oneshot::channel();
                    self.rmc_tx
                        .send(RMCMessage::GetRoomByKey(room_id.clone(), tx))
                        .await
                        .unwrap();
                    let room = rx.await.unwrap();
                    if let Some(room) = room {
                        self.rmc_tx
                            .send(RMCMessage::SendSessionMessage(
                                room.owner.wsid.clone(),
                                SendMessage::OnMessage(self.wsid.clone(), message),
                            ))
                            .await
                            .unwrap();
                    }
                }
            }
            RecvMessage::Status(new_status) => {
                self.state.status = Some(new_status);
                self.rmc_tx
                    .send(RMCMessage::UpdateSessionState(
                        self.wsid.clone(),
                        self.state.clone(),
                    ))
                    .await
                    .unwrap();
                self.rmc_tx
                    .send(RMCMessage::BroadcastMessage(SendMessage::UpdateClients))
                    .await
                    .unwrap();
            }
            RecvMessage::Close(key) => {
                let (tx, rx) = oneshot::channel();
                self.rmc_tx
                    .send(RMCMessage::GetSessionState(key.clone(), tx))
                    .await
                    .unwrap();
                if let Some(state) = rx.await.unwrap() {
                    if let Some(room_id) = state.room_id {
                        let (tx, rx) = oneshot::channel();
                        self.rmc_tx
                            .send(RMCMessage::GetRoomByKey(room_id.clone(), tx))
                            .await
                            .unwrap();
                        let room = rx.await.unwrap();
                        if let Some(room) = room {
                            if room.owner.wsid == self.wsid {
                                self.rmc_tx
                                    .send(RMCMessage::CloseSession(key.clone()))
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                }
            }
            _ => {
                self.close().await;
                todo!("not implemented: {:?}", msg);
            }
        }
    }

    async fn close(&mut self) {
        if let Some(room_id) = &self.state.room_id {
            let (tx, rx) = oneshot::channel();
            self.rmc_tx
                .send(RMCMessage::GetRoomByKey(room_id.clone(), tx))
                .await
                .unwrap();
            if let Some(room) = rx.await.unwrap() {
                if room.owner.wsid == self.wsid {
                    // 若我是房主
                    let (tx, rx) = oneshot::channel();
                    self.rmc_tx
                        .send(RMCMessage::GetRoomPlayersByKey(room_id.clone(), tx))
                        .await
                        .unwrap();
                    let sessions = rx.await.unwrap();
                    for session in sessions {
                        if session != self.wsid {
                            self.rmc_tx
                                .send(RMCMessage::SendSessionMessage(
                                    session,
                                    SendMessage::SelfClose,
                                ))
                                .await
                                .unwrap();
                        }
                    }
                } else {
                    self.rmc_tx
                        .send(RMCMessage::SendSessionMessage(
                            room.owner.wsid.clone(),
                            SendMessage::OnClose(self.wsid.clone()),
                        ))
                        .await
                        .unwrap();
                }
            }
        }
        self.rmc_tx
            .send(RMCMessage::RemoveSession(self.wsid.clone()))
            .await
            .unwrap();
        self.rmc_tx
            .send(RMCMessage::RemoveRoom(self.wsid.clone()))
            .await
            .unwrap();
        self.rmc_tx
            .send(RMCMessage::BroadcastMessage(SendMessage::UpdateRooms))
            .await
            .unwrap();
        self.control_tx.send(ControlMessage::Close).await.unwrap();
    }

    async fn send_(ws: &mut WebsocketSender, text: &str) -> Result<(), WebSocketError> {
        println!("<- {}", text);
        ws.write_frame(Frame::text(fastwebsockets::Payload::Borrowed(
            text.as_bytes(),
        )))
        .await
    }

    async fn read_(ws: &mut WebsocketReader) -> Result<Frame, WebSocketError> {
        ws.read_frame::<_, WebSocketError>(&mut move |_| async { Ok(()) })
            .await
    }
}
