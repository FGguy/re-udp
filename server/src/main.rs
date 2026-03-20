use std::net::UdpSocket;

use clap::Parser;

mod cli;

fn main() {
    let args = cli::Args::parse();
    let ip_addr = format!("127.0.0.1:{}", args.port);

    let mut _receive_buf = [1];

    let _socket  = UdpSocket::bind(&ip_addr).unwrap_or_else(|e| {
        panic!("Failed to bind UDP socket to: {}, error: {}", &ip_addr, e)
    });
}
