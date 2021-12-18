use std::{
    os::unix::{
        net::{UnixListener, UnixStream},
        prelude::AsRawFd,
    },
    path::Path,
    sync::Arc,
    thread::{self, JoinHandle},
};

use crate::Export;
use anyhow::Result;
use nbd::{client::Client, Server};

pub fn start_unix_socket_server(export: &Export, path: &Path) -> Result<()> {
    let server: Arc<Server<UnixStream>> = Arc::new(nbd::Server::new(export.clone()));
    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    let listener = UnixListener::bind(path)?;
    for conn in listener.incoming() {
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
                eprintln!("error: {}", e)
            }
        }
    }

    handles.into_iter().for_each(|h| h.join().unwrap());

    Ok(())
}
