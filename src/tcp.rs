use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{self, atomic::AtomicBool, Arc},
    thread::{self, sleep, JoinHandle},
    time::Duration,
};

use crate::{client::Client, Export, Server};
use anyhow::Result;

pub fn start_tcp_server(export: &Export, address: SocketAddr, stop: &AtomicBool) -> Result<()> {
    let server: Arc<Server<TcpStream>> = Arc::new(Server::new(export.clone()));
    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    let listener = TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;

    for conn in listener.incoming() {
        if stop.load(sync::atomic::Ordering::SeqCst) {
            println!("Received stop signal, exiting");
            break;
        }

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
                match e.kind() {
                    io::ErrorKind::WouldBlock => {
                        sleep(Duration::from_millis(100));
                        continue;
                    }
                    _ => {
                        eprintln!("error: {}", e);
                    }
                }
                eprintln!("error: {}", e)
            }
        }
    }

    handles.into_iter().for_each(|h| h.join().unwrap());

    Ok(())
}
