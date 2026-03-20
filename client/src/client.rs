use std::error;
use std::net;

use rand::Rng;

use shared::{MessageType, ReUDPPacket, RequestPayload};

enum ClientState {
    Request,
    Receiving,
}

pub struct Client {
    state: ClientState,
    socket: net::UdpSocket,

    /// byte are pushed to this buffer until the full file has been received
    /// once all the bytes are received, the content of this buffer is converted to a file
    file_buffer: Vec<u8>,

    receive_buffer: [u8; 65507],
    file_name: String,
    segment_size: u32,
    server_ip_addr: String,
}

impl Client {
    pub fn new(
        socket: net::UdpSocket,
        file_name: String,
        segment_size: u32,
        server_ip_addr: String,
    ) -> Client {
        Client {
            state: ClientState::Request,
            socket: socket,
            file_buffer: Vec::new(),
            receive_buffer: [0u8; 65507],
            file_name: file_name,
            segment_size: segment_size,
            server_ip_addr: server_ip_addr,
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn error::Error>> {
        loop {
            match self.state {
                ClientState::Request => {
                    self.socket
                        .connect(format!("{}:9090", self.server_ip_addr))?;

                    let payload = RequestPayload {
                        file_name: self.file_name.to_owned(),
                        segment_size: self.segment_size.to_owned(),
                    };

                    let serialized_payload = bincode::serialize(&payload)?;
                    let payload_length = serialized_payload.len();

                    let packet = ReUDPPacket {
                        connection_id: rand::rng().random_range(0..16),
                        sequence_number: 0,
                        message_type: MessageType::Request,
                        payload_length,
                        payload: serialized_payload,
                    };

                    let serialized_packet = bincode::serialize(&packet)?;

                    self.socket.send(&serialized_packet)?;
                    self.state = ClientState::Receiving;
                }

                ClientState::Receiving => {
                    let (_, _) = self.socket.recv_from(&mut self.file_buffer)?;
                    let _packet: ReUDPPacket = bincode::deserialize(&self.file_buffer)?;
                }
            }
        }
        Ok(())
    }
}
