use clap::Parser;
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
                    Ok(nbd::InteractionResult::Abort) => {
                        println!("Handshake aborted");
                        stream.shutdown(std::net::Shutdown::Both).unwrap();
                        continue;
                    }
                    Ok(nbd::InteractionResult::Continue) => {
                        println!("Starting transmission");
                    }
                    Err(e) => {
                        eprintln!("Encountered error, shutting down stream: {}", e);
                        stream.shutdown(std::net::Shutdown::Both).unwrap();

                        continue;
                    }
                }

                match server.transmission(&client) {
                    Ok(nbd::InteractionResult::Continue) => {}
                    Ok(nbd::InteractionResult::Abort) => {
                        println!("Transmission aborted");
                        stream.shutdown(std::net::Shutdown::Both).unwrap();

                        continue;
                    }
                    Err(e) => {
                        eprintln!("Encountered error, shutting down stream: {}", e);
                        stream.shutdown(std::net::Shutdown::Both).unwrap();

                        continue;
                    }
                }
            }
            Err(e) => {
                eprintln!("error: {}", e)
            }
        }
    }
}
