use clap::Parser;

/// Client command line arguments
#[derive(Parser, Debug)]
#[command(version, about = None, long_about = None)]
pub struct Args {
    /// Port the client will use for its UDP socket
    #[arg(short, long)]
    pub port: u16,

    /// IP address of the server from which the file is to be retrieved
    #[arg(long = "ip-addr", default_value_t = String::from("127.0.0.1"))]
    pub ip_addr: String,

    /// Name of the file to retrieve
    #[arg(short, long = "file-name", required = true)]
    pub file_name: String,

    /// Size of the payload in bytes
    #[arg(short, long = "segment-size", default_value_t = 512)]
    pub segment_size: u32,
}
