#[derive(Debug)]
pub enum RecvMessage {
    Create(String, String, String),
    Enter(String, String, String),
    ChangeAvatar(String, String),
    _Server,
    Key(String),
    _Events,
    Config(String),
    Status(String),
    Send(String, String),
    Close(String),
    SessionClose,
    ForwardToOwner(String),
    Heartbeat,
    PlaceHolder
}

#[derive(Clone, Debug)]
pub enum SendMessage {
    CreateRoom,
    EnterRoomFailed,
    OnConnection(String),
    _ReloadRoom,
    _Denied,
    _EventDenied,
    UpdateRooms,
    UpdateClients,
    _UpdateEvents,
    RoomList,
    OnMessage(String, String),
    SelfClose,
    OnClose(String),
    Heartbeat,
    Plain(String),
}

pub enum ControlMessage {
    Send(SendMessage),
    Heartbeat,
    Close
}