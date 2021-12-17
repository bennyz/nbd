use clap::Parser;
use nbd::client::Client;
use nbd::{self, Export, Server};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::prelude::AsRawFd;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};

#[derive(Parser, Clone)]
#[clap(version = "0.0.1")]
struct Args {
    /// Sets a custom config file. Could have been an Option<T> with no default too
    file: String,

    #[clap(default_value = "")]
    name: String,

    #[clap(default_value = "")]
    description: String,

    #[clap(long)]
    unix: bool,
}

fn main() {
    let args = Args::parse();
    if !Path::exists(Path::new(&args.file)) {
        panic!("{} does not exist!", args.file);
    }

    let export = Arc::new(RwLock::new(Export {
        name: args.name,
        description: args.description,
        path: args.file,
        multiconn: true,
        ..Default::default()
    }));

    export.write().unwrap().init_export().unwrap();
    let clone = &Arc::clone(&export);
    let mut handlers: Vec<JoinHandle<()>> = Vec::new();
    if args.unix {
        println!("Starting unix socket server");
        let export = clone.read().unwrap();
        let server: Arc<Server<UnixStream>> = Arc::new(nbd::Server::new(export.clone()));

        // TODO: make UNIX socket configurable
        std::fs::remove_file("/tmp/nbd.sock").ok();
        let listener = UnixListener::bind("/tmp/nbd.sock").unwrap();

        let handle = thread::spawn(move || {
            for conn in listener.incoming() {
                println!("got incoming!");
                match conn {
                    Ok(stream) => {
                        let fd = &stream.as_raw_fd();
                        let client = Client::new(stream, format!("unix-sock-{}", fd));
                        let clone = Arc::clone(&server);
                        thread::spawn(move || {
                            clone.handle(client).unwrap();
                        });
                    }
                    Err(e) => {
                        eprintln!("error: {}", e)
                    }
                }
            }
        });
        handlers.push(handle);
    }

    // Make backends for each export selectable
    println!("Starting tcp server");
    let clone = &Arc::clone(&export);
    let export = clone.read().unwrap();

    let server: Arc<Server<TcpStream>> = Arc::new(nbd::Server::new(export.clone()));

    let listener =
        TcpListener::bind(format!("127.0.0.1:{}", nbd::consts::NBD_DEFAULT_PORT)).unwrap();
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let client_addr = stream.peer_addr().unwrap().to_string();
                let client = Client::new(stream, client_addr);
                let clone = Arc::clone(&server);
                thread::spawn(move || {
                    clone.handle(client).unwrap();
                });
            }
            Err(e) => {
                eprintln!("error: {}", e)
            }
        }
    }

    handlers.into_iter().for_each(|h| h.join().unwrap());
}
