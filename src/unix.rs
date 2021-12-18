use crate::Export;
use anyhow::Result;
use nbd::{client::Client, Server};
use std::{
    io,
    os::unix::{
        net::{UnixListener, UnixStream},
        prelude::AsRawFd,
    },
    path::Path,
    sync::{self, atomic::AtomicBool, Arc},
    thread::{self, sleep, JoinHandle},
    time::Duration,
};

pub fn start_unix_socket_server(export: &Export, path: &Path, stop: &AtomicBool) -> Result<()> {
    let server: Arc<Server<UnixStream>> = Arc::new(nbd::Server::new(export.clone()));
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
                let client = Client::new(stream, format!("unix-sock-{}", fd));
                let clone = Arc::clone(&server);
                handles.push(thread::spawn(move || {
                    clone.handle(client).unwrap();
                }));
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

    // Maybe this can be done automatically somehow?
    println!("Cleaning up UNIX socket: {}", path.to_str().unwrap());
    std::fs::remove_file(path)?;
    Ok(())
}
