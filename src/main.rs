use std::net::{TcpListener, TcpStream};

use nbd::{self, Server};

fn main() {
    let listener =
        TcpListener::bind(format!("127.0.0.1:{}", nbd::consts::NBD_DEFAULT_PORT)).unwrap();
    let mut server: Server<TcpStream> = nbd::Server::new();
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let client = stream.peer_addr().unwrap().to_string();
                println!("Incoming connection from: {}", &client);
                server.add_connection(client.to_owned(), stream).unwrap();
                server.negotiate(&client).unwrap();
            }
            Err(e) => {
                eprintln!("error: {}", e)
            }
        }
    }
}
