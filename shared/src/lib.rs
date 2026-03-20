use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum MessageType {
    Request,
    Data,
    Ack,
    Error
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReUDPPacket {
    pub connection_id: u16,
    pub sequence_number: u8,
    pub message_type: MessageType,
    pub payload_length: u64,
    pub payload: Vec<u8>
}
