use clap::Parser;

/// Client command line arguments
#[derive(Parser, Debug)]
#[command(version, about = None, long_about = None)]
pub struct Args {
    /// Port the server will use for its UDP socket
    #[arg(short, long)]
    pub port: u16,

    /// Folder which contains the files this server can serve
    #[arg(short, long = "file-directory", default_value_t = String::from("/"))]
    pub file_directory: String
}