use nbd::{self, Server};
use std::net::{TcpListener, TcpStream};

fn main() {
    let listener =
        TcpListener::bind(format!("127.0.0.1:{}", nbd::consts::NBD_DEFAULT_PORT)).unwrap();
    let mut server: Server<TcpStream> = nbd::Server::new();
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
