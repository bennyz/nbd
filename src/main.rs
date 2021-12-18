use clap::Parser;
use nbd::{self, Export};
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::unix::start_unix_socket_server;

mod tcp;
mod unix;

#[derive(Parser, Clone)]
#[clap(version = "0.0.1")]
struct Args {
    /// The file we want to export
    file: String,

    /// The name of the export, empty by default
    #[clap(default_value = "")]
    name: String,

    /// The description of the export, empty by default
    #[clap(default_value = "")]
    description: String,

    /// Whether to use a UNIX socket (additionally) along with the TCP socket
    /// by default uses /tmp/nbd.sock, in the future it will be configurable
    #[clap(long)]
    unix: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !Path::exists(Path::new(&args.file)) {
        panic!("{} does not exist!", args.file);
    }

    let export = Arc::new(RwLock::new(Export::init_export(
        args.file,
        args.name,
        args.description,
    )?));

    let clone = &Arc::clone(&export);
    if args.unix {
        println!("Listening on UNIX socket {}", "/tmp/nbd.sock");
        start_unix_socket_server(&clone.read().unwrap(), Path::new("/tmp/nbd.sock"))?;
    }

    // Make backends for each export selectable
    println!("Listening on port {}", nbd::consts::NBD_DEFAULT_PORT);
    let export = export.read().unwrap();

    tcp::start_tcp_server(
        &export,
        format!("127.0.0.1:{}", nbd::consts::NBD_DEFAULT_PORT).parse()?,
    )?;

    Ok(())
}
