use clap::Parser;
use nbd::{self, Export, Server};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

#[derive(Parser, Clone)]
#[clap(version = "0.0,.")]
struct Opts {
    /// Sets a custom config file. Could have been an Option<T> with no default too
    file: String,

    #[clap(default_value = "")]
    name: String,

    #[clap(default_value = "")]
    description: String,
}

fn main() {
    let opts = Opts::parse();
    if !Path::exists(Path::new(&opts.file)) {
        panic!("{} does not exist!", opts.file);
    }

    let export = Export {
        name: opts.name,
        description: opts.description,
        path: opts.file,
        read_only: true,
        ..Default::default()
    };

    let mut server: Server<TcpStream> = nbd::Server::new(export);

    let listener =
        TcpListener::bind(format!("127.0.0.1:{}", nbd::consts::NBD_DEFAULT_PORT)).unwrap();
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let client = stream.peer_addr().unwrap().to_string();
                server
                    .add_connection(client.to_owned(), stream.try_clone().unwrap())
                    .unwrap();
                match server.handshake(&client) {
                    Ok(nbd::HandshakeResult::Abort) => {
                        println!("Handshake aborted");
                        continue;
                    }
                    Ok(nbd::HandshakeResult::Continue) => {
                        println!("Starting transmission");
                    }
                    Err(e) => {
                        eprintln!("Encountered error, shutting down stream: {}", e);
                    }
                }
                if let Err(e) = server.transmission(&client) {
                    eprintln!("Encountered error, shutting down stream: {}", e);
                    stream.shutdown(std::net::Shutdown::Both).unwrap();
                }
            }
            Err(e) => {
                eprintln!("error: {}", e)
            }
        }
    }
}
