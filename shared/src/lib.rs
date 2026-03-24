use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MessageType {
    Request,
    Data,
    Ack,
    Error,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReUDPPacket {
    pub connection_id: u16,
    pub sequence_number: u8,
    pub message_type: MessageType,
    pub payload_length: usize,
    pub payload: Vec<u8>,
    pub is_final: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestPayload {
    pub file_name: String,
    pub segment_size: u32,
}
