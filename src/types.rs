use core::fmt;
use fastwebsockets::FragmentCollectorRead;

use crate::message::ControlMessage;

pub struct SessionItem {
    pub controller: ControlChannelSender,
    pub state: SessionState,
}

#[derive(Clone)]
pub struct SessionState {
    pub nickname: Option<String>,
    pub avatar: Option<String>,
    pub room_id: Option<String>,
    pub status: Option<String>,
    pub wsid: String,
    pub online_key: Option<String>,
}

impl SessionState {
    pub fn new(wsid: String) -> SessionState {
        SessionState {
            nickname: None,
            avatar: None,
            online_key: None,
            room_id: None,
            status: None,
            wsid,
        }
    }
}

#[derive(Clone)]
pub struct Room {
    pub people_number: u32,
    pub owner: SessionState,
    pub key: String,
    pub config: Option<String>,
}

impl Room {
    pub fn new(owner: SessionState, key: String) -> Room {
        Room {
            people_number: 1,
            owner,
            key,
            config: None,
        }
    }
}

impl fmt::Debug for Room {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nickname = none2null(&self.owner.nickname, true);
        let avatar = none2null(&self.owner.avatar, true);
        let config = none2null(&self.config, false);
        write!(
            f,
            r#"[{},{},{},{},{}]"#,
            nickname, avatar, config, self.people_number, self.key
        )
    }
}

fn none2null(text: &Option<String>, quote: bool) -> String {
    match text {
        Some(s) => {
            if quote {
                format!("\"{}\"", s)
            } else {
                s.to_string()
            }
        }
        None => "null".to_string(),
    }
}
impl fmt::Debug for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = none2null(&self.status, true);
        let nickname = none2null(&self.nickname, true);
        let avatar = none2null(&self.avatar, true);
        let online_key = none2null(&self.online_key, false);
        let in_room = self.room_id.is_some();
        write!(
            f,
            r#"[{},{},{},{},"{}",{}]"#,
            nickname, avatar, !in_room, status, self.wsid, online_key
        )
    }
}

pub type WebsocketSender = fastwebsockets::WebSocketWrite<
    tokio::io::WriteHalf<hyper_util::rt::TokioIo<hyper::upgrade::Upgraded>>,
>;
pub type WebsocketReader =
    FragmentCollectorRead<tokio::io::ReadHalf<hyper_util::rt::TokioIo<hyper::upgrade::Upgraded>>>;
pub type ControlChannelSender = tokio::sync::mpsc::Sender<ControlMessage>;
pub type ControlChannelReceiver = tokio::sync::mpsc::Receiver<ControlMessage>;
