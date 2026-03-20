use std::error;
use std::net;

enum ServerState {
    WaitingForConnection,
    Error(String),
}

pub struct Server {
    state: ServerState,
    socket: net::UdpSocket,
    receive_buffer: [u8; 65507],
}

impl Server {
    pub fn new(socket: net::UdpSocket) -> Server {
        Server {
            state: ServerState::WaitingForConnection,
            socket: socket,
            receive_buffer: [0u8; 65507],
        }
    }

    pub fn run(&self) -> Result<(), Box<dyn error::Error>> {
        loop {
            match self.state {
                ServerState::WaitingForConnection => unimplemented!(),
                _ => break,
            }
        }
        Ok(())
    }
}
