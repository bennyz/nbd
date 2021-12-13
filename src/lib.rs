use anyhow::Result;
use bincode::config::Configuration;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use client::Client;
use consts::{
    NbdReply, NBD_FLAG_C_FIXED_NEWSTYLE, NBD_FLAG_C_NO_ZEROES, NBD_FLAG_FIXED_NEWSTYLE,
    NBD_FLAG_HAS_FLAGS, NBD_FLAG_NO_ZEROES, NBD_REP_MAGIC, NBD_SIMPLE_REPLY_MAGIC,
};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::fs::{self, OpenOptions};
use std::intrinsics::transmute;
use std::io::{Read, Write};
use std::os::unix::prelude::{FileExt, MetadataExt};
use std::path::Path;
use thiserror::Error;

use crate::consts::{
    NbdCmd, NbdInfoOpt, NbdOpt, MAX_BLOCK_SIZE, MIN_BLOCK_SIZE, NBD_INIT_MAGIC, NBD_OPTS_MAGIC,
    NBD_REQUEST_MAGIC, NBD_REQUEST_SIZE, PREFERRED_BLOCK_SIZE,
};

pub mod client;
pub mod consts;

const EMPTY_REPLY: &[u8; 0] = b"";

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

#[derive(Debug, Default)]
pub struct Export {
    pub path: String,
    pub name: String,
    pub description: String,
    pub size: u64,
    pub read_only: bool,
    pub can_resize: bool,
    pub fast_zero: bool,
    pub trim: bool,
    pub flush: bool,
    pub rotational: bool,
    pub df: bool,
    pub multiconn: bool,
}

impl Export {
    pub fn init_export(&mut self) -> Result<(), anyhow::Error> {
        let path = Path::new(&self.path);
        let md = fs::metadata(path)?;
        self.size = md.size();
        println!("md size {}", self.size);
        Ok(())
    }
}

#[derive(Debug, bincode::Encode, bincode::Decode)]
#[repr(C)]
struct OptionReply {
    magic: u64,
    option: u32,
    reply_type: u32,
    length: u32,
}

// NBD client request
// #define NBD_REQUEST_SIZE            (4 + 2 + 2 + 8 + 8 + 4)
#[derive(Debug, bincode::Encode, bincode::Decode)]
#[repr(C)]
struct Request {
    magic: u32,
    flags: u16,
    command_type: u16,
    handle: u64,
    offset: u64,
    len: u32,
}

#[derive(Debug, Default)]
pub struct Server<T: Read + Write> {
    clients: HashMap<String, Client<T>>,
    export: Export,
}

impl<T> Server<T>
where
    T: Read + Write,
{
    pub fn new(export: Export) -> Self {
        let clients = HashMap::new();
        Server { clients, export }
    }

    pub fn handle(&mut self, c: Client<T>) -> Result<(), anyhow::Error> {
        self.export.init_export().unwrap();
        let addr = c.addr().to_owned();
        self.add_connection(c).unwrap();
        let client = self.clients.get(&addr).unwrap();
        self.handshake(&client).unwrap();
        self.transmission(&client).unwrap();

        Ok(())
    }

    fn handshake(&self, c: &Client<T>) -> Result<InteractionResult, Box<dyn Error>> {
        // 64 bits
        c.stream()
            .borrow_mut()
            .write_all(&NBD_INIT_MAGIC.to_be_bytes())?;

        // 64 bits
        c.stream()
            .borrow_mut()
            .write_all(&NBD_OPTS_MAGIC.to_be_bytes())?;

        // 16 bits
        let handshake_flags = NBD_FLAG_FIXED_NEWSTYLE | NBD_FLAG_NO_ZEROES;

        c.stream()
            .borrow_mut()
            .write_u16::<BigEndian>(handshake_flags)?;
        c.stream().borrow_mut().flush()?;

        // Start reading client negotiation
        // option flags
        let client_flags = c.stream().borrow_mut().read_u32::<BigEndian>()?;
        println!("Received client flags: {:#02x}", client_flags);
        if client_flags != NBD_FLAG_C_FIXED_NEWSTYLE
            && client_flags != (NBD_FLAG_C_FIXED_NEWSTYLE | NBD_FLAG_C_NO_ZEROES)
        {
            eprintln!("Unknown client flags {:#02x}", client_flags);
        }

        loop {
            // Check client magic
            let client_magic = c.stream().borrow_mut().read_u64::<BigEndian>()?;
            println!("Checking opts magic: {:#02x}", client_magic);
            if client_magic != NBD_OPTS_MAGIC {
                eprintln!(
                    "Bad magic received {:#02x}, expected {:#02x}",
                    client_magic, NBD_OPTS_MAGIC
                );
                continue;
            }

            // Read option
            let option = c.stream().borrow_mut().read_u32::<BigEndian>()?;
            println!("Checking option {:#02x}", option);

            // Read option length
            let option_length = c.stream().borrow_mut().read_u32::<BigEndian>()?;
            println!("Received option length {}", option_length);

            // TODO: Remove later
            let option: NbdOpt = unsafe { transmute(option) };

            match option {
                NbdOpt::Export => {
                    self.handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::ExportName => {
                    println!("Received EXPORT_NAME option");
                    c.stream()
                        .borrow_mut()
                        .write_u64::<BigEndian>(self.export.size)?;

                    // TODO use a sane way to initialize the flags
                    let mut flags: u16 = 0;
                    set_flags(&self.export, &mut flags);
                    c.stream().borrow_mut().write_u16::<BigEndian>(flags)?;
                    c.stream().borrow_mut().flush()?;
                }
                NbdOpt::List => {
                    self.handle_list(c, &self.export.name, &self.export.description)?;
                }
                NbdOpt::Abort => {
                    println!("Aborting");
                    if self
                        .handshake_reply(c, option, NbdReply::Ack, EMPTY_REPLY)
                        .is_err()
                    {
                        eprintln!("Ignoring abort ACK errors");
                    }
                    return Ok(InteractionResult::Abort);
                }
                NbdOpt::StructuredReply => {
                    self.handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                opt @ NbdOpt::Info => {
                    println!("Received info");
                    self.handle_export_info(c, opt)?;
                }
                opt @ NbdOpt::Go => {
                    println!("Received go");
                    self.handle_export_info(c, opt)?;
                    return Ok(InteractionResult::Continue);
                }
                NbdOpt::ListMetaContext => {
                    self.handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::SetMetaContext => {
                    self.handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::StartTls => {
                    self.handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
            }
        }
    }

    fn transmission(&self, c: &Client<T>) -> Result<InteractionResult> {
        let mut opts = OpenOptions::new();
        opts.read(true);
        if !self.export.read_only {
            opts.write(true);
        }

        let file = &opts.open(&self.export.path)?;

        let mut request_buf: [u8; NBD_REQUEST_SIZE as usize] = [0; NBD_REQUEST_SIZE as usize];
        loop {
            c.stream().borrow_mut().read_exact(&mut request_buf)?;

            let request: Request = bincode::decode_from_slice(
                &request_buf,
                Configuration::standard()
                    .with_big_endian()
                    .with_fixed_int_encoding(),
            )?;

            println!("Checking opts magic: {:?}", request.magic);
            if request.magic != NBD_REQUEST_MAGIC {
                eprintln!(
                    "Bad magic received {:#02x}, expected {:#02x}",
                    request.magic, NBD_REQUEST_MAGIC
                );

                return Err(anyhow::anyhow!(NbdError::BadMagic(request.magic as usize)));
            }

            let cmd: NbdCmd = unsafe { transmute(request.command_type) };
            match cmd {
                NbdCmd::Read => {
                    println!(
                        "Received read request, len {}, offset {}",
                        request.len, request.offset
                    );

                    let mut buf: Vec<u8> = vec![0; request.len as usize];
                    let read = file.read_at(buf.as_mut_slice(), request.offset)?;
                    println!("Read {} bytes", read);
                    self.transmission_simple_reply_header(c, request.handle, 0)?;

                    c.stream().borrow_mut().write_all(&buf)?;
                    c.stream().borrow_mut().flush()?;
                }
                NbdCmd::Write => {
                    println!(
                        "Received write request, len {}, offset {}",
                        request.len, request.offset
                    );

                    let mut buf: Vec<u8> = vec![0; request.len as usize];
                    c.stream().borrow_mut().read_exact(buf.as_mut_slice())?;
                    file.write_at(&buf, request.offset)?;
                    self.transmission_simple_reply_header(c, request.handle, 0)?;
                    c.stream().borrow_mut().flush()?;
                }
                NbdCmd::Disc => {
                    println!("Disconnect requested");
                    c.stream().borrow_mut().flush()?;
                    return Ok(InteractionResult::Abort);
                }
                NbdCmd::Flush => {
                    println!("Received flush");
                    c.stream().borrow_mut().flush()?;
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

    fn add_connection(&mut self, client: Client<T>) -> Result<(), Box<dyn Error>> {
        self.clients.insert(client.addr().to_owned(), client);

        Ok(())
    }

    fn handle_export_info(&self, c: &Client<T>, opt: NbdOpt) -> Result<(), Box<dyn Error>> {
        // Read name length
        let len = c.stream().borrow_mut().read_u32::<BigEndian>()?;
        println!("Received length {}", len);
        let mut buf: Vec<u8> = vec![0; len as usize];

        // Read name
        c.stream().borrow_mut().read_exact(buf.as_mut_slice())?;
        let export_name = String::from_utf8(buf.clone())?;
        println!("Received export name {}", export_name);

        // Read number of requests
        let requests = c.stream().borrow_mut().read_u16::<BigEndian>()?;
        println!("Receiving {} request(s)", requests);

        let mut send_name = false;
        let mut send_description = false;

        for i in 0..requests {
            // TODO use proper safe conversion
            let option = unsafe { transmute(c.stream().borrow_mut().read_u16::<BigEndian>()?) };
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
            self.info_reply(
                c,
                opt,
                NbdInfoOpt::Name,
                (self.export.name.len() + 2) as u32,
                self.export.name.as_bytes(),
            )?;
        }

        if send_description {
            self.info_reply(
                c,
                opt,
                NbdInfoOpt::Description,
                (self.export.description.len() + 2) as u32,
                self.export.description.as_bytes(),
            )?;
        }

        let sizes: Vec<u32> = vec![
            MIN_BLOCK_SIZE as u32,
            PREFERRED_BLOCK_SIZE as u32,
            std::cmp::min(self.export.size, MAX_BLOCK_SIZE) as u32,
        ];

        println!("Reporting sizes {:?}", sizes);

        self.info_reply(
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
        set_flags(&self.export, &mut flags);

        println!(
            "Sending export '{}' information, flags {}",
            self.export.name, flags
        );
        self.info_reply(c, opt, NbdInfoOpt::Export, 12, EMPTY_REPLY)?;

        c.stream()
            .borrow_mut()
            .write_all(&self.export.size.to_be_bytes())?;
        c.stream().borrow_mut().write_all(&flags.to_be_bytes())?;
        c.stream().borrow_mut().flush()?;

        self.handshake_reply(c, opt, NbdReply::Ack, EMPTY_REPLY)?;

        Ok(())
    }

    // TODO: support multiple (and actual) exports
    fn handle_list(
        &self,
        c: &Client<T>,
        name: &str,
        description: &str,
    ) -> Result<(), Box<dyn Error>> {
        let reply_header = OptionReply {
            magic: NBD_REP_MAGIC,
            option: (NbdOpt::List as u32),
            reply_type: (NbdReply::Server as u32),

            // Why +4? size of the length field (32)
            length: (name.len() as u32 + description.len() as u32 + 4),
        };

        self.header_reply(c, reply_header)?;
        c.stream()
            .borrow_mut()
            .write_all(&(name.len() as u32).to_be_bytes())?;
        c.stream().borrow_mut().write_all(name.as_bytes())?;
        c.stream().borrow_mut().write_all(description.as_bytes())?;
        c.stream().borrow_mut().flush()?;

        self.handshake_reply(c, NbdOpt::List, NbdReply::Ack, EMPTY_REPLY)?;

        Ok(())
    }

    fn info_reply(
        &self,
        c: &Client<T>,
        opt: NbdOpt,
        info_type: NbdInfoOpt,
        len: u32,
        data: &[u8],
    ) -> Result<(), Box<dyn Error>> {
        let header = OptionReply {
            magic: NBD_REP_MAGIC,
            option: (opt as u32),
            reply_type: (NbdReply::Info as u32),
            length: len,
        };

        c.stream().borrow_mut().write_all(&bincode::encode_to_vec(
            &header,
            Configuration::standard()
                .with_big_endian()
                .with_fixed_int_encoding(),
        )?)?;
        c.stream()
            .borrow_mut()
            .write_u16::<BigEndian>(info_type as u16)?;

        // Send payload
        if data != EMPTY_REPLY {
            c.stream().borrow_mut().write_all(data)?;
        }
        c.stream().borrow_mut().flush()?;

        Ok(())
    }

    fn header_reply(&self, c: &Client<T>, header: OptionReply) -> Result<(), Box<dyn Error>> {
        let serialized = bincode::encode_to_vec(
            &header,
            Configuration::standard()
                .with_big_endian()
                .with_fixed_int_encoding(),
        )?;
        dbg!(&serialized);
        c.stream().borrow_mut().write_all(&serialized)?;
        c.stream().borrow_mut().flush()?;

        Ok(())
    }

    fn handshake_reply(
        &self,
        c: &Client<T>,
        client_option: NbdOpt,
        reply_type: NbdReply,
        data: &[u8],
    ) -> Result<(), Box<dyn Error>> {
        c.stream()
            .borrow_mut()
            .write_u64::<BigEndian>(NBD_REP_MAGIC)?;
        c.stream()
            .borrow_mut()
            .write_u32::<BigEndian>(client_option as u32)?;
        c.stream()
            .borrow_mut()
            .write_u32::<BigEndian>(reply_type as u32)?;
        c.stream()
            .borrow_mut()
            .write_u32::<BigEndian>(data.len() as u32)?;
        c.stream().borrow_mut().write_all(data)?;
        c.stream().borrow_mut().flush()?;

        Ok(())
    }

    fn transmission_simple_reply_header(
        &self,
        c: &Client<T>,
        handle: u64,
        error: u32,
    ) -> Result<()> {
        c.stream()
            .borrow_mut()
            .write_u32::<BigEndian>(NBD_SIMPLE_REPLY_MAGIC)?;
        c.stream().borrow_mut().write_u32::<BigEndian>(error)?;
        c.stream().borrow_mut().write_u64::<BigEndian>(handle)?;

        Ok(())
    }
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
