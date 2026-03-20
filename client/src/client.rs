use std::net;
use std::error;

enum ClientState {
    Connecting
}

pub struct Client {
    state: ClientState,
    socket: net::UdpSocket,
    receive_buffer: [u8; 65507]
}

impl Client {
    pub fn new(socket: net::UdpSocket) -> Client {
        Client{ state: ClientState::Connecting, socket: socket, receive_buffer: [0u8; 65507] }
    }

    pub fn run(&self) -> Result<(), Box<dyn error::Error>> {
        loop {
            match self.state {
                ClientState::Connecting => break,
            }
        }
        Ok(())
    }
}