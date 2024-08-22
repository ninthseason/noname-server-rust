use crate::{
    message::{ControlMessage, SendMessage},
    Room, SessionItem, SessionState,
};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

pub struct ResourceManager {
    rooms: HashMap<String, Room>,
    sessions: HashMap<String, SessionItem>,
}

impl ResourceManager {
    pub fn new() -> Self {
        ResourceManager {
            rooms: HashMap::new(),
            sessions: HashMap::new(),
        }
    }

    pub async fn start(&mut self, mut control_rx: mpsc::Receiver<RMCMessage>) {
        loop {
            if let Some(message) = control_rx.recv().await {
                match message {
                    RMCMessage::InsertRoom(key, room) => {
                        self.rooms.insert(key, room);
                    }
                    RMCMessage::GetRoom(key, tx) => {
                        tx.send(self.rooms.get(&key).cloned()).unwrap();
                    }
                    RMCMessage::GetRoomByKey(key, tx) => {
                        if self.rooms.is_empty() {
                            tx.send(None).unwrap();
                        } else {
                            for room in self.rooms.values() {
                                if room.key == key {
                                    tx.send(Some(room.clone())).unwrap();
                                    break;
                                }
                            }
                        }
                    }
                    RMCMessage::GetRoomList(tx) => {
                        tx.send(format!("{:?}", self.rooms.values().filter(|x| x.config.is_some()).collect::<Vec<&Room>>()))
                            .unwrap();
                    }
                    RMCMessage::GetRoomPlayersByKey(key, tx) => {
                        let mut result = vec![];
                        for session in self.sessions.values() {
                            if let Some(room_id) = &session.state.room_id {
                                if room_id == &key {
                                    result.push(session.state.wsid.clone());
                                }
                            }
                        }
                        tx.send(result).unwrap();
                    }
                    RMCMessage::RemoveRoom(key) => {
                        self.rooms.remove(&key);
                    }
                    RMCMessage::InsertSession(key, session) => {
                        self.sessions.insert(key, session);
                    }
                    RMCMessage::RemoveSession(key) => {
                        self.sessions.remove(&key);
                    }
                    RMCMessage::GetSessionList(tx) => {
                        let sessions = self
                            .sessions
                            .values()
                            .map(|x| &x.state)
                            .collect::<Vec<&SessionState>>();
                        tx.send(format!("{:?}", sessions)).unwrap();
                    }
                    RMCMessage::CloseSession(key) => {
                        if let Some(session) = self.sessions.get(&key) {
                            let _ = session.controller.send(ControlMessage::Close).await;
                        }
                    }
                    RMCMessage::UpdateSessionState(key, state) => {
                        if let Some(session) = self.sessions.get_mut(&key) {
                            session.state = state;
                        }
                    }
                    RMCMessage::GetSessionState(key, tx) => {
                        tx.send(self.sessions.get(&key).map(|x| x.state.clone()))
                            .unwrap();
                    }
                    RMCMessage::SendSessionMessage(key, message) => {
                        if let Some(session) = self.sessions.get_mut(&key) {
                            let rc = session.controller.send(ControlMessage::Send(message)).await;
                            if rc.is_err() {
                                println!("[RM]Error sending message to session {}", key);
                            }
                        }
                    }
                    RMCMessage::BroadcastMessage(message) => {
                        for session in self.sessions.values_mut() {
                            let rc = session.controller.send(ControlMessage::Send(message.clone())).await;
                            if rc.is_err() {
                                println!("Error sending message to session {}", session.state.wsid);
                            }
                        }
                    }
                    RMCMessage::_Close => break,
                }
            }
        }
        println!("Resource Manager closed");
    }
}

pub enum RMCMessage {
    // Resource Manager Control Message
    InsertRoom(String, Room),
    GetRoom(String, oneshot::Sender<Option<Room>>),
    GetRoomByKey(String, oneshot::Sender<Option<Room>>),
    GetRoomList(oneshot::Sender<String>),
    GetRoomPlayersByKey(String, oneshot::Sender<Vec<String>>),
    RemoveRoom(String),
    InsertSession(String, SessionItem),
    UpdateSessionState(String, SessionState),
    GetSessionState(String, oneshot::Sender<Option<SessionState>>),
    RemoveSession(String),
    GetSessionList(oneshot::Sender<String>),
    SendSessionMessage(String, SendMessage),
    CloseSession(String),
    BroadcastMessage(SendMessage),
    _Close,
}
