use std::error;
use std::fs;
use std::io;
use std::net;
use std::time::Duration;

use rand::Rng;

use shared::{MessageType, ReUDPPacket, RequestPayload};

// Arbitrary timeout and retry limits for the client when waiting for a response to its REQUEST.
const REQUEST_TIMEOUT_MS: u64 = 500;
const MAX_REQUEST_RETRIES: u32 = 10;

enum ClientState {
    Requesting,
    Receiving,
}

pub struct Client {
    state: ClientState,
    socket: net::UdpSocket,

    /// Bytes are pushed here until the full file has been received,
    /// then written to disk.
    file_buffer: Vec<u8>,

    receive_buffer: [u8; 65507],
    file_name: String,
    segment_size: u32,
    server_ip_addr: String,
    server_port: u16,
    connection_id: u16,
    expected_seq: u8,
    request_retries: u32,
}

impl Client {
    pub fn new(
        socket: net::UdpSocket,
        file_name: String,
        segment_size: u32,
        server_ip_addr: String,
        server_port: u16,
    ) -> Client {
        Client {
            state: ClientState::Requesting,
            socket,
            file_buffer: Vec::new(),
            receive_buffer: [0u8; 65507],
            file_name,
            segment_size,
            server_ip_addr,
            server_port,
            connection_id: 0,
            expected_seq: 0,
            request_retries: 0,
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn error::Error>> {
        loop {
            match self.state {
                ClientState::Requesting => {
                    // "Connect" the UDP socket to the server
                    // is used to not have to pass the server address to send() every time
                    self.socket
                        .connect(format!("{}:{}", self.server_ip_addr, self.server_port))?;

                    // Generate random connection ID
                    let conn_id = rand::rng().random::<u16>();
                    self.connection_id = conn_id;

                    // Create, serialize, and send REQUEST packet
                    let payload = RequestPayload {
                        file_name: self.file_name.clone(),
                        segment_size: self.segment_size,
                    };
                    let serialized_payload = bincode::serialize(&payload)?;
                    let payload_length = serialized_payload.len();

                    let packet = ReUDPPacket {
                        connection_id: conn_id,
                        sequence_number: 0,
                        message_type: MessageType::Request,
                        payload_length,
                        payload: serialized_payload,
                        is_final: false,
                    };

                    self.socket.send(&bincode::serialize(&packet)?)?;
                    println!(
                        "[client] Sent REQUEST for '{}' conn_id={}",
                        self.file_name, conn_id
                    );

                    self.expected_seq = 0;
                    self.request_retries = 0;
                    self.socket
                        .set_read_timeout(Some(Duration::from_millis(REQUEST_TIMEOUT_MS)))?;

                    // Set state to Receiving so we can wait for the server's response (DATA or ERROR)
                    self.state = ClientState::Receiving;
                }

                ClientState::Receiving => {
                    let n = match self.socket.recv(&mut self.receive_buffer) {
                        Ok(n) => n,
                        Err(ref e)
                            if e.kind() == io::ErrorKind::WouldBlock
                                || e.kind() == io::ErrorKind::TimedOut =>
                        {
                            // No response received, retransmit the REQUEST if we haven't
                            // yet received any data for this transfer.
                            if self.file_buffer.is_empty() && self.expected_seq == 0 {
                                self.request_retries += 1;
                                if self.request_retries > MAX_REQUEST_RETRIES {
                                    return Err(format!(
                                        "No response from server after {} retries",
                                        MAX_REQUEST_RETRIES
                                    )
                                    .into());
                                }
                                println!(
                                    "[client] Timeout waiting for response, retrying REQUEST (attempt {})",
                                    self.request_retries
                                );
                                self.state = ClientState::Requesting;
                            }
                            continue;
                        }
                        Err(e) => return Err(Box::new(e)),
                    };

                    let packet: ReUDPPacket = match bincode::deserialize(&self.receive_buffer[..n])
                    {
                        Ok(p) => p,
                        Err(_) => {
                            eprintln!("[client] Malformed packet, ignoring");
                            continue;
                        }
                    };

                    if packet.connection_id != self.connection_id {
                        println!(
                            "[client] Discarding packet with wrong conn_id={}",
                            packet.connection_id
                        );
                        continue;
                    }

                    match packet.message_type {
                        MessageType::Error => {
                            return Err("Server returned an error (file not found?)".into());
                        }

                        MessageType::Data => {
                            if packet.sequence_number != self.expected_seq {
                                // Duplicate or out-of-order DATA, re-ACK it so the server
                                // knows we already have this one and can move on.
                                println!(
                                    "[client] Wrong seq={} (expected {}), re-ACKing seq={}",
                                    packet.sequence_number,
                                    self.expected_seq,
                                    packet.sequence_number
                                );
                                let ack = ReUDPPacket {
                                    connection_id: self.connection_id,
                                    sequence_number: packet.sequence_number,
                                    message_type: MessageType::Ack,
                                    payload_length: 0,
                                    payload: Vec::new(),
                                    is_final: false,
                                };
                                self.socket.send(&bincode::serialize(&ack)?)?;
                                continue;
                            }

                            println!(
                                "[client] Received DATA seq={} ({} bytes) is_final={}",
                                packet.sequence_number, packet.payload_length, packet.is_final
                            );

                            self.file_buffer.extend_from_slice(&packet.payload);

                            // Send ACK.
                            let ack = ReUDPPacket {
                                connection_id: self.connection_id,
                                sequence_number: self.expected_seq,
                                message_type: MessageType::Ack,
                                payload_length: 0,
                                payload: Vec::new(),
                                is_final: false,
                            };
                            self.socket.send(&bincode::serialize(&ack)?)?;
                            println!("[client] Sent ACK seq={}", self.expected_seq);

                            self.expected_seq ^= 1;

                            if packet.is_final {
                                println!(
                                    "[client] Transfer complete. Writing {} bytes to '{}'",
                                    self.file_buffer.len(),
                                    self.file_name
                                );
                                fs::write(&self.file_name, &self.file_buffer)?;
                                println!("[client] File saved as '{}'", self.file_name);
                                return Ok(());
                            }
                        }

                        _ => {
                            println!("[client] Unexpected message type, ignoring");
                        }
                    }
                }
            }
        }
    }
}
