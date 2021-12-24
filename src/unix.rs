use crate::{client::Client, Export, Server};
use anyhow::Result;

use std::{
    io,
    os::unix::{net::UnixListener, prelude::AsRawFd},
    path::Path,
    sync::{self, atomic::AtomicBool, Arc},
    thread::{self, sleep, JoinHandle},
    time::Duration,
};

pub fn start_unix_socket_server(export: &Export, path: &Path, stop: &AtomicBool) -> Result<()> {
    let server: Arc<Server> = Arc::new(Server::new(export.clone()));
    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    let listener = UnixListener::bind(path)?;
    listener.set_nonblocking(true)?;

    for conn in listener.incoming() {
        if stop.load(sync::atomic::Ordering::SeqCst) {
            println!("Received stop signal, exiting");
            break;
        }

        match conn {
            Ok(stream) => {
                let fd = &stream.as_raw_fd();
                let mut client = Client::new(stream, format!("unix-sock-{}", fd));
                let clone = Arc::clone(&server);
                let h = thread::spawn(move || {
                    if let Err(e) = clone.handle(&mut client) {
                        eprintln!("Error handling client: {}", e);
                    }
                });
                handles.push(h);
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
    handles.into_iter().for_each(|h| {
        if h.join().is_err() {
            println!("Thread panicked");
        }
    });

    // Maybe this can be done automatically somehow?
    println!("Cleaning up UNIX socket: {}", path.to_str().unwrap());
    std::fs::remove_file(path)?;
    Ok(())
}
