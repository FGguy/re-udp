use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum MessageType {
    Request,
    Data,
    Ack,
    Error,
    End,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReUDPPacket {
    pub connection_id: u16,
    pub sequence_number: u8,
    pub message_type: MessageType,
    pub payload_length: usize,
    pub payload: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestPayload {
    pub file_name: String,
    pub segment_size: u32,
}
