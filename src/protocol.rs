use derive_more::From;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

pub trait ReqRes {
    type Response: Serialize;
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Greeting {
    pub protocol_version: u32,
}

#[derive(Serialize, Deserialize)]
pub enum GreetingResponse {
    ProtocolOk,
    UnsupportedProtocol,
}

impl ReqRes for Greeting {
    type Response = GreetingResponse;
}

#[derive(Serialize, Deserialize, Debug, From)]
pub enum ClientMessage {
    Greeting(Greeting),
    #[from(ignore)]
    Disconnect,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerResponse {}
