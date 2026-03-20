use std::net::UdpSocket;

use clap::Parser;

mod cli;
mod client;

fn main() {
    let args = cli::Args::parse();
    let ip_addr = format!("127.0.0.1:{}", args.port);

    let socket = UdpSocket::bind(&ip_addr)
        .unwrap_or_else(|e| panic!("Failed to bind UDP socket to: {}, error: {}", &ip_addr, e));

    let mut client = client::Client::new(socket, args.file_name, args.segment_size, args.ip_addr);
    if let Err(e) = client.run() {
        panic!("Client error: {}", e)
    }
}
