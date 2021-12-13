use clap::Parser;
use nbd::client::Client;
use nbd::{self, Export, Server};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

#[derive(Parser, Clone)]
#[clap(version = "0.0.1")]
struct Args {
    /// Sets a custom config file. Could have been an Option<T> with no default too
    file: String,

    #[clap(default_value = "")]
    name: String,

    #[clap(default_value = "")]
    description: String,
}

fn main() {
    let args = Args::parse();
    if !Path::exists(Path::new(&args.file)) {
        panic!("{} does not exist!", args.file);
    }

    let export = Export {
        name: args.name,
        description: args.description,
        path: args.file,
        multiconn: true,
        ..Default::default()
    };

    let mut server: Server<TcpStream> = nbd::Server::new(export);

    let listener =
        TcpListener::bind(format!("127.0.0.1:{}", nbd::consts::NBD_DEFAULT_PORT)).unwrap();
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let client_addr = stream.peer_addr().unwrap().to_string();
                let client = Client::new(stream, client_addr);
                server.handle(client).unwrap();
            }
            Err(e) => {
                eprintln!("error: {}", e)
            }
        }
    }
}
