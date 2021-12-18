use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    thread::{self, JoinHandle},
};

use crate::Export;
use anyhow::Result;
use nbd::{client::Client, Server};

pub fn start_tcp_server(export: &Export, address: SocketAddr) -> Result<()> {
    let server: Arc<Server<TcpStream>> = Arc::new(nbd::Server::new(export.clone()));
    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    let listener = TcpListener::bind(address)?;
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let client_addr = stream.peer_addr().unwrap().to_string();
                let client = Client::new(stream, client_addr);
                let clone = Arc::clone(&server);
                let join_handle = thread::spawn(move || {
                    clone.handle(client).unwrap();
                });

                handles.push(join_handle);
            }
            Err(e) => {
                eprintln!("error: {}", e)
            }
        }
    }

    handles.into_iter().for_each(|h| h.join().unwrap());

    Ok(())
}
