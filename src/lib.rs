use anyhow::Result;
use bincode::config::Configuration;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use client::Client;
use consts::{
    NbdReply, NBD_FLAG_C_FIXED_NEWSTYLE, NBD_FLAG_C_NO_ZEROES, NBD_FLAG_FIXED_NEWSTYLE,
    NBD_FLAG_HAS_FLAGS, NBD_FLAG_NO_ZEROES,
};

use std::fmt::Debug;
use std::fs::{self, OpenOptions};
use std::intrinsics::transmute;
use std::io::{Read, Write};
use std::os::unix::prelude::MetadataExt;
use std::path::Path;

use thiserror::Error;

use crate::consts::{
    NbdCmd, NbdInfoOpt, NbdOpt, MAX_BLOCK_SIZE, MIN_BLOCK_SIZE, NBD_INIT_MAGIC, NBD_OPTS_MAGIC,
    NBD_REQUEST_MAGIC, NBD_REQUEST_SIZE, PREFERRED_BLOCK_SIZE,
};

pub mod client;
pub mod consts;
mod protocol;
pub mod tcp;
pub mod unix;

#[derive(Debug, Error)]
pub enum NbdError {
    #[error("Bad Magic Number: {0}")]
    BadMagic(usize),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub enum InteractionResult {
    Abort,
    Continue,
}

#[derive(Debug, Clone)]
pub struct Export {
    path: String,
    name: String,
    description: String,
    size: u64,
    read_only: bool,
    can_resize: bool,
    fast_zero: bool,
    trim: bool,
    flush: bool,
    rotational: bool,
    df: bool,
    multiconn: bool,
}

impl Export {
    pub fn init_export(path: String, name: String, description: String) -> Result<Export> {
        let md = fs::metadata(Path::new(&path))?;

        let export = Export {
            path,
            name,
            description,
            size: md.size(),
            read_only: false, // TODO make configurable
            can_resize: false,
            fast_zero: false,
            trim: false,
            flush: false,
            rotational: false,
            df: true,
            multiconn: true,
        };

        Ok(export)
    }
}

#[derive(Debug)]
pub struct Server {
    export: Export,
}

impl Server {
    pub fn new(export: Export) -> Self {
        Server { export }
    }

    pub fn handle<T: Read + Write>(&self, c: &mut Client<T>) -> Result<()> {
        let addr = c.addr().to_owned();
        println!("Handling client {}", addr);

        match self.handshake(c, &self.export)? {
            InteractionResult::Abort => {
                println!("Aborting connection");
                return Ok(());
            }
            InteractionResult::Continue => {
                println!("Continuing connection");
            }
        }

        println!("Starting transmission");
        match self.transmission(c, &self.export)? {
            InteractionResult::Abort => {
                println!("Aborting connection");
                return Ok(());
            }
            InteractionResult::Continue => {
                println!("Continuing connection");
            }
        }

        Ok(())
    }

    fn handshake<T: Read + Write>(
        &self,
        c: &mut Client<T>,
        export: &Export,
    ) -> Result<InteractionResult> {
        // 64 bits
        c.stream().write_all(&NBD_INIT_MAGIC.to_be_bytes())?;

        // 64 bits
        c.stream().write_all(&NBD_OPTS_MAGIC.to_be_bytes())?;

        // 16 bits
        let handshake_flags = NBD_FLAG_FIXED_NEWSTYLE | NBD_FLAG_NO_ZEROES;

        c.stream().write_u16::<BigEndian>(handshake_flags)?;
        c.stream().flush()?;

        // Start reading client negotiation
        // option flags
        let client_flags = c.stream().read_u32::<BigEndian>()?;
        println!("Received client flags: {:#02x}", client_flags);
        if client_flags != NBD_FLAG_C_FIXED_NEWSTYLE
            && client_flags != (NBD_FLAG_C_FIXED_NEWSTYLE | NBD_FLAG_C_NO_ZEROES)
        {
            eprintln!("Unknown client flags {:#02x}", client_flags);
        }

        loop {
            // Check client magic
            let client_magic = c.stream().read_u64::<BigEndian>()?;
            println!("Checking opts magic: {:#02x}", client_magic);
            if client_magic != NBD_OPTS_MAGIC {
                eprintln!(
                    "Bad magic received {:#02x}, expected {:#02x}",
                    client_magic, NBD_OPTS_MAGIC
                );
                continue;
            }

            // Read option
            let option = c.stream().read_u32::<BigEndian>()?;
            println!("Checking option {:#02x}", option);

            // Read option length
            let option_length = c.stream().read_u32::<BigEndian>()?;
            println!("Received option length {}", option_length);

            let mut option_data = vec![0; option_length as usize];
            c.read_exact(&mut option_data)?;
            println!("Read option data {:?}", option_data);
            // TODO: Remove later
            let option: NbdOpt = unsafe { transmute(option) };

            match option {
                NbdOpt::Export => {
                    protocol::handshake_reply(
                        c,
                        option,
                        NbdReply::NbdRepErrUnsup,
                        protocol::EMPTY_REPLY,
                    )?;
                }
                NbdOpt::ExportName => {
                    println!("Received EXPORT_NAME option");
                    c.stream().write_u64::<BigEndian>(export.size)?;

                    // TODO use a sane way to initialize the flags
                    let mut flags: u16 = 0;
                    set_flags(export, &mut flags);
                    c.stream().write_u16::<BigEndian>(flags)?;
                    c.stream().flush()?;
                }
                NbdOpt::List => {
                    protocol::handle_list(c, &export.name, &export.description)?;
                }
                NbdOpt::Abort => {
                    println!("Aborting");
                    if protocol::handshake_reply(c, option, NbdReply::Ack, protocol::EMPTY_REPLY)
                        .is_err()
                    {
                        eprintln!("Ignoring abort ACK errors");
                    }
                    return Ok(InteractionResult::Abort);
                }
                NbdOpt::StructuredReply => {
                    c.set_structured_reply(true);
                    protocol::handshake_reply(c, option, NbdReply::Ack, protocol::EMPTY_REPLY)?;
                }
                opt @ NbdOpt::Info => {
                    println!("Received info");
                    handle_export_info(c, opt, export, &option_data)?;
                }
                opt @ NbdOpt::Go => {
                    println!("Received go");
                    handle_export_info(c, opt, export, &option_data)?;
                    return Ok(InteractionResult::Continue);
                }
                NbdOpt::ListMetaContext => {
                    protocol::handshake_reply(
                        c,
                        option,
                        NbdReply::NbdRepErrUnsup,
                        protocol::EMPTY_REPLY,
                    )?;
                }
                NbdOpt::SetMetaContext => {
                    protocol::handshake_reply(
                        c,
                        option,
                        NbdReply::NbdRepErrUnsup,
                        protocol::EMPTY_REPLY,
                    )?;
                }
                NbdOpt::StartTls => {
                    protocol::handshake_reply(
                        c,
                        option,
                        NbdReply::NbdRepErrUnsup,
                        protocol::EMPTY_REPLY,
                    )?;
                }
            }
        }
    }

    fn transmission<T: Read + Write>(
        &self,
        c: &mut Client<T>,
        export: &Export,
    ) -> Result<InteractionResult> {
        println!("Opening export file {}", export.path);
        let mut opts = OpenOptions::new();
        opts.read(true);
        if !export.read_only {
            opts.write(true);
        }

        let file = &opts.open(&export.path)?;

        let mut request_buf: [u8; NBD_REQUEST_SIZE as usize] = [0; NBD_REQUEST_SIZE as usize];
        loop {
            let read = c.stream().read(&mut request_buf)?;
            println!("Read {} bytes", read);

            if (read as u32) < NBD_REQUEST_SIZE {
                eprintln!("Invalid request size");
                return Ok(InteractionResult::Abort);
            }

            let request: protocol::Request = bincode::decode_from_slice(
                &request_buf,
                Configuration::standard()
                    .with_big_endian()
                    .with_fixed_int_encoding(),
            )?
            .0;

            println!("Checking opts magic: {:?}", request.magic);
            if request.magic != NBD_REQUEST_MAGIC {
                eprintln!(
                    "Bad magic received {:#02x}, expected {:#02x}",
                    request.magic, NBD_REQUEST_MAGIC
                );

                continue;
            }

            let cmd: NbdCmd = unsafe { transmute(request.command_type) };
            match cmd {
                NbdCmd::Read => {
                    println!(
                        "Received read request, len {}, offset {}",
                        request.len, request.offset
                    );
                    protocol::do_read(c, request.handle, request.offset, request.len, file)?;
                }
                NbdCmd::Write => {
                    println!(
                        "Received write request, len {}, offset {}",
                        request.len, request.offset
                    );

                    protocol::do_write(c, request.handle, request.offset, request.len, file)?;
                }
                NbdCmd::Disc => {
                    println!("Disconnect requested");
                    c.stream().flush()?;
                    return Ok(InteractionResult::Abort);
                }
                NbdCmd::Flush => {
                    println!("Received flush");
                    c.stream().flush()?;
                }
                NbdCmd::Trim => {
                    println!("trim!");
                }
                NbdCmd::Cache => {
                    println!("cache!");
                }
                NbdCmd::WriteZeroes => {
                    println!("write zeroes!");
                }
                NbdCmd::BlockStatus => {
                    println!("block status!");
                }
            }
        }
    }
}

fn handle_export_info<T: Read + Write>(
    c: &mut Client<T>,
    opt: NbdOpt,
    export: &Export,
    data: &[u8],
) -> Result<()> {
    // Read number of requests
    let requests = u16::from_be_bytes(data[0..2].try_into().unwrap());
    println!("Receiving {} request(s)", requests);

    let mut send_name = false;
    let mut send_description = false;

    for i in 0..requests {
        // TODO use proper safe conversion
        let option = unsafe { transmute(c.stream().read_u16::<BigEndian>()?) };
        println!("Request {}/{}, option {:?}", i + 1, requests, option);

        match option {
            NbdInfoOpt::Export => {
                println!("Sending export info");
            }
            NbdInfoOpt::Name => {
                println!("export name requested");
                send_name = true;
            }
            NbdInfoOpt::Description => {
                println!("export description requested");
                send_description = true;
            }
            NbdInfoOpt::BlockSize => {
                println!("block size requested");
            }
            NbdInfoOpt::Unknown => {
                panic!("Shouldn't happen");
            }
        }
    }

    if send_name {
        protocol::info_reply(
            c,
            opt,
            NbdInfoOpt::Name,
            (export.name.len() + 2) as u32,
            export.name.as_bytes(),
        )?;
    }

    if send_description {
        protocol::info_reply(
            c,
            opt,
            NbdInfoOpt::Description,
            (export.description.len() + 2) as u32,
            export.description.as_bytes(),
        )?;
    }

    let sizes: Vec<u32> = vec![
        MIN_BLOCK_SIZE as u32,
        PREFERRED_BLOCK_SIZE as u32,
        std::cmp::min(export.size, MAX_BLOCK_SIZE) as u32,
    ];

    println!("Reporting sizes {:?}", sizes);

    protocol::info_reply(
        c,
        opt,
        NbdInfoOpt::BlockSize,
        14,
        &sizes
            .iter()
            .flat_map(|x| x.to_be_bytes())
            .collect::<Vec<u8>>(),
    )?;

    let mut flags: u16 = 0;
    set_flags(export, &mut flags);

    println!(
        "Sending export '{}' information, flags {}",
        export.name, flags
    );
    protocol::info_reply(c, opt, NbdInfoOpt::Export, 12, protocol::EMPTY_REPLY)?;

    c.stream().write_all(&export.size.to_be_bytes())?;
    c.stream().write_all(&flags.to_be_bytes())?;
    c.stream().flush()?;

    protocol::handshake_reply(c, opt, NbdReply::Ack, protocol::EMPTY_REPLY)?;

    Ok(())
}

fn set_flags(export: &Export, flags: &mut u16) {
    *flags |= NBD_FLAG_HAS_FLAGS;
    if export.read_only {
        *flags |= consts::NBD_FLAG_READ_ONLY;
    }
    if export.can_resize {
        *flags |= consts::NBD_FLAG_SEND_FLUSH;
    }
    if export.fast_zero {
        *flags |= consts::NBD_FLAG_SEND_FAST_ZERO;
    }
    if export.rotational {
        *flags |= consts::NBD_FLAG_ROTATIONAL;
    }
    if export.trim {
        *flags |= consts::NBD_FLAG_SEND_TRIM;
    }
    if export.flush {
        *flags |= consts::NBD_FLAG_SEND_FLUSH;
    }
    if export.df {
        *flags |= consts::NBD_FLAG_SEND_DF;
    }
    if export.multiconn {
        *flags |= consts::NBD_FLAG_CAN_MULTI_CONN;
    }
}
