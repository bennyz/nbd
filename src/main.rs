use clap::Parser;
use nbd::{self, Export, Server};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

#[derive(Parser, Clone)]
#[clap(version = "0.0,.")]
struct Opts {
    /// Sets a custom config file. Could have been an Option<T> with no default too
    file: String,
    name: String,
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
                if let Err(e) = server.negotiate(&client) {
                    println!("Encountered error, shutting down stream: {}", e);
                    stream.shutdown(std::net::Shutdown::Both).unwrap();
                }
            }
            Err(e) => {
                eprintln!("error: {}", e)
            }
        }
    }
}
