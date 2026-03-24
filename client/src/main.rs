use std::net::UdpSocket;

use clap::Parser;

mod cli;
mod client;

// Localhost is 0.0.0.0 on Linux
const LOCALHOST: &str = "0.0.0.0";

fn main() {
    // Parse command line arguments
    let args = cli::Args::parse();
    let ip_addr = format!("{}:{}", LOCALHOST, args.port);

    // Create UDP socket and bind to the specified port
    let socket = UdpSocket::bind(&ip_addr)
        .unwrap_or_else(|e| panic!("Failed to bind UDP socket to: {}, error: {}", &ip_addr, e));

    // Create client and run
    let mut client = client::Client::new(
        socket,
        args.file_name,
        args.segment_size,
        args.ip_addr,
        args.server_port,
    );
    if let Err(e) = client.run() {
        panic!("Client error: {}", e)
    }
}
