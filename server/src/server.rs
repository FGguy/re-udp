use std::error;
use std::fs;
use std::io;
use std::net;
use std::path::Path;
use std::time::{Duration, Instant};

use shared::{MessageType, ReUDPPacket, RequestPayload};

const MAX_RETRANSMITS: u32 = 10;

// Start with a reasonable default before we have any RTT samples.
const INITIAL_TIMEOUT_MS: u64 = 500;

enum ServerState {
    WaitingForConnection,
    Sending {
        client_addr: net::SocketAddr,
        connection_id: u16,
        segments: Vec<Vec<u8>>,
        current_index: usize,
        current_seq: u8,
        retransmit_count: u32,
        // Adaptive timeout fields.
        // avg_rtt_ms is a running EWMA of observed round-trip times.
        // timeout_ms is what we actually pass to set_read_timeout; it gets
        // doubled on each retransmit and recalculated after a clean ACK.
        avg_rtt_ms: f64,
        timeout_ms: u64,
        send_time: Instant,
    },
}

pub struct Server {
    state: ServerState,
    socket: net::UdpSocket,
    receive_buffer: [u8; 65507],
    file_directory: String,
}

impl Server {
    pub fn new(socket: net::UdpSocket, file_directory: String) -> Server {
        Server {
            state: ServerState::WaitingForConnection,
            socket,
            receive_buffer: [0u8; 65507],
            file_directory,
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn error::Error>> {
        loop {
            let next = self.step()?;
            self.state = next;
        }
    }

    fn step(&mut self) -> Result<ServerState, Box<dyn error::Error>> {
        // Take ownership of the current state so we can move out of it freely.
        let state = std::mem::replace(&mut self.state, ServerState::WaitingForConnection);

        match state {
            ServerState::WaitingForConnection => {
                self.socket.set_read_timeout(None)?;
                let (n, src) = self.socket.recv_from(&mut self.receive_buffer)?;

                let packet: ReUDPPacket = match bincode::deserialize(&self.receive_buffer[..n]) {
                    Ok(p) => p,
                    Err(_) => {
                        eprintln!("[server] Malformed packet from {}, ignoring", src);
                        return Ok(ServerState::WaitingForConnection);
                    }
                };

                if !matches!(packet.message_type, MessageType::Request) {
                    return Ok(ServerState::WaitingForConnection);
                }

                let req: RequestPayload = match bincode::deserialize(&packet.payload) {
                    Ok(r) => r,
                    Err(_) => {
                        eprintln!("[server] Malformed REQUEST payload from {}, ignoring", src);
                        return Ok(ServerState::WaitingForConnection);
                    }
                };

                println!(
                    "[server] REQUEST from {} conn_id={} file='{}' seg_size={}",
                    src, packet.connection_id, req.file_name, req.segment_size
                );

                let file_path = Path::new(&self.file_directory).join(&req.file_name);
                let file_data = match fs::read(&file_path) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("[server] Cannot read file {:?}: {}", file_path, e);
                        let err_pkt = ReUDPPacket {
                            connection_id: packet.connection_id,
                            sequence_number: 0,
                            message_type: MessageType::Error,
                            payload_length: 0,
                            payload: Vec::new(),
                            is_final: false,
                        };
                        let bytes = bincode::serialize(&err_pkt)?;
                        let _ = self.socket.send_to(&bytes, src);
                        return Ok(ServerState::WaitingForConnection);
                    }
                };

                let seg_size = req.segment_size as usize;
                let seg_size = if seg_size == 0 { 512 } else { seg_size };

                let segments: Vec<Vec<u8>> = if file_data.is_empty() {
                    vec![Vec::new()]
                } else {
                    file_data.chunks(seg_size).map(|c| c.to_vec()).collect()
                };

                println!(
                    "[server] {} bytes -> {} segment(s) of up to {} bytes",
                    file_data.len(),
                    segments.len(),
                    seg_size
                );

                // Send the first segment immediately.
                let is_final = segments.len() == 1;
                let first_seg = segments[0].clone();
                let first_len = first_seg.len();
                let data_pkt = ReUDPPacket {
                    connection_id: packet.connection_id,
                    sequence_number: 0,
                    message_type: MessageType::Data,
                    payload_length: first_len,
                    payload: first_seg,
                    is_final,
                };
                let bytes = bincode::serialize(&data_pkt)?;
                self.socket.send_to(&bytes, src)?;
                println!(
                    "[server] Sent DATA seq=0 ({} bytes) is_final={}",
                    first_len, is_final
                );

                self.socket
                    .set_read_timeout(Some(Duration::from_millis(INITIAL_TIMEOUT_MS)))?;

                Ok(ServerState::Sending {
                    client_addr: src,
                    connection_id: packet.connection_id,
                    segments,
                    current_index: 0,
                    current_seq: 0,
                    retransmit_count: 0,
                    avg_rtt_ms: INITIAL_TIMEOUT_MS as f64 / 2.0,
                    timeout_ms: INITIAL_TIMEOUT_MS,
                    send_time: Instant::now(),
                })
            }

            ServerState::Sending {
                client_addr,
                connection_id,
                segments,
                current_index,
                current_seq,
                retransmit_count,
                avg_rtt_ms,
                timeout_ms,
                send_time,
            } => {
                let total = segments.len();
                let is_final = current_index == total - 1;
                let current_seg = segments[current_index].clone();

                match self.socket.recv_from(&mut self.receive_buffer) {
                    Err(ref e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::TimedOut =>
                    {
                        if retransmit_count >= MAX_RETRANSMITS {
                            eprintln!(
                                "[server] Max retransmits ({}) reached, dropping conn_id={}",
                                MAX_RETRANSMITS, connection_id
                            );
                            return Ok(ServerState::WaitingForConnection);
                        }

                        // Double the timeout on each retransmit (exponential backoff).
                        let new_timeout_ms = (timeout_ms * 2).min(10_000);
                        println!(
                            "[server] Timeout -- retransmitting DATA seq={} (attempt {}), next timeout={}ms",
                            current_seq,
                            retransmit_count + 1,
                            new_timeout_ms,
                        );
                        self.socket
                            .set_read_timeout(Some(Duration::from_millis(new_timeout_ms)))?;

                        let data_pkt = ReUDPPacket {
                            connection_id,
                            sequence_number: current_seq,
                            message_type: MessageType::Data,
                            payload_length: current_seg.len(),
                            payload: current_seg,
                            is_final,
                        };
                        let bytes = bincode::serialize(&data_pkt)?;
                        self.socket.send_to(&bytes, client_addr)?;

                        Ok(ServerState::Sending {
                            client_addr,
                            connection_id,
                            segments,
                            current_index,
                            current_seq,
                            retransmit_count: retransmit_count + 1,
                            avg_rtt_ms,
                            timeout_ms: new_timeout_ms,
                            // Reset the send time so that if the retransmit eventually
                            // gets ACKed we don't measure a wildly inflated RTT.
                            send_time: Instant::now(),
                        })
                    }

                    Err(e) => Err(Box::new(e)),

                    Ok((n, src)) => {
                        // Ignore packets from unexpected sources.
                        if src != client_addr {
                            return Ok(ServerState::Sending {
                                client_addr,
                                connection_id,
                                segments,
                                current_index,
                                current_seq,
                                retransmit_count,
                                avg_rtt_ms,
                                timeout_ms,
                                send_time,
                            });
                        }

                        let pkt: ReUDPPacket =
                            match bincode::deserialize(&self.receive_buffer[..n]) {
                                Ok(p) => p,
                                Err(_) => {
                                    return Ok(ServerState::Sending {
                                        client_addr,
                                        connection_id,
                                        segments,
                                        current_index,
                                        current_seq,
                                        retransmit_count,
                                        avg_rtt_ms,
                                        timeout_ms,
                                        send_time,
                                    });
                                }
                            };

                        if pkt.connection_id != connection_id
                            || !matches!(pkt.message_type, MessageType::Ack)
                        {
                            return Ok(ServerState::Sending {
                                client_addr,
                                connection_id,
                                segments,
                                current_index,
                                current_seq,
                                retransmit_count,
                                avg_rtt_ms,
                                timeout_ms,
                                send_time,
                            });
                        }

                        if pkt.sequence_number != current_seq {
                            // Duplicate ACK for the previous packet — retransmit current DATA.
                            println!(
                                "[server] Duplicate ACK seq={} (expected {}), retransmitting",
                                pkt.sequence_number, current_seq
                            );
                            let data_pkt = ReUDPPacket {
                                connection_id,
                                sequence_number: current_seq,
                                message_type: MessageType::Data,
                                payload_length: current_seg.len(),
                                payload: current_seg,
                                is_final,
                            };
                            let bytes = bincode::serialize(&data_pkt)?;
                            self.socket.send_to(&bytes, client_addr)?;
                            return Ok(ServerState::Sending {
                                client_addr,
                                connection_id,
                                segments,
                                current_index,
                                current_seq,
                                retransmit_count,
                                avg_rtt_ms,
                                timeout_ms,
                                send_time,
                            });
                        }

                        // Correct ACK received.
                        // Only update the RTT estimate when this was a clean (non-retransmitted)
                        // send, otherwise the sample is ambiguous.
                        let (new_avg_rtt_ms, new_timeout_ms) = if retransmit_count == 0 {
                            let rtt_ms = send_time.elapsed().as_secs_f64() * 1_000.0;
                            // Exponential moving average: weight new sample at 1/4.
                            let updated_avg = 0.75 * avg_rtt_ms + 0.25 * rtt_ms;
                            // Timeout = 2× the average RTT, clamped to a sensible range.
                            let updated_timeout = ((updated_avg * 2.0) as u64).clamp(100, 10_000);
                            println!(
                                "[server] ACK seq={} OK  rtt={:.1}ms  avg_rtt={:.1}ms  timeout={}ms",
                                current_seq, rtt_ms, updated_avg, updated_timeout
                            );
                            (updated_avg, updated_timeout)
                        } else {
                            println!("[server] ACK seq={} OK (after {} retransmit(s))", current_seq, retransmit_count);
                            (avg_rtt_ms, timeout_ms)
                        };

                        if is_final {
                            println!(
                                "[server] Transfer complete for conn_id={}",
                                connection_id
                            );
                            return Ok(ServerState::WaitingForConnection);
                        }

                        // Advance to the next segment.
                        let next_index = current_index + 1;
                        let next_seq = current_seq ^ 1;
                        let next_is_final = next_index == total - 1;
                        let next_seg = segments[next_index].clone();
                        let next_len = next_seg.len();

                        let data_pkt = ReUDPPacket {
                            connection_id,
                            sequence_number: next_seq,
                            message_type: MessageType::Data,
                            payload_length: next_len,
                            payload: next_seg,
                            is_final: next_is_final,
                        };
                        let bytes = bincode::serialize(&data_pkt)?;
                        self.socket.send_to(&bytes, client_addr)?;
                        println!(
                            "[server] Sent DATA seq={} ({} bytes) is_final={}",
                            next_seq, next_len, next_is_final
                        );

                        self.socket
                            .set_read_timeout(Some(Duration::from_millis(new_timeout_ms)))?;

                        Ok(ServerState::Sending {
                            client_addr,
                            connection_id,
                            segments,
                            current_index: next_index,
                            current_seq: next_seq,
                            retransmit_count: 0,
                            avg_rtt_ms: new_avg_rtt_ms,
                            timeout_ms: new_timeout_ms,
                            send_time: Instant::now(),
                        })
                    }
                }
            }
        }
    }
}
