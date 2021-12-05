use bincode::config::Configuration;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use consts::{
    NbdReply, NBD_FLAG_C_FIXED_NEWSTYLE, NBD_FLAG_C_NO_ZEROES, NBD_FLAG_FIXED_NEWSTYLE,
    NBD_FLAG_HAS_FLAGS, NBD_FLAG_NO_ZEROES, NBD_REP_MAGIC, NBD_SIMPLE_REPLY_MAGIC,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::fs::{self, File};
use std::intrinsics::transmute;
use std::io::{Read, Write};
use std::os::unix::prelude::{FileExt, MetadataExt};
use std::path::Path;
use std::rc::Rc;

use crate::consts::{
    NbdCmd, NbdInfoOpt, NbdOpt, MAX_BLOCK_SIZE, MIN_BLOCK_SIZE, NBD_INIT_MAGIC, NBD_OPTS_MAGIC,
    NBD_REQUEST_MAGIC, NBD_REQUEST_SIZE, PREFERRED_BLOCK_SIZE,
};

pub mod consts;

const EMPTY_REPLY: &[u8; 0] = b"";

pub enum HandshakeResult {
    Abort,
    Continue,
}

#[derive(Debug, Default)]
pub struct Server<T: Read + Write + Debug> {
    clients: Rc<RefCell<HashMap<String, T>>>,
    export: Export,
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
}

impl Export {
    pub fn init_flags(&mut self) -> Result<(), Box<dyn Error>> {
        let path = Path::new(&self.path);
        let md = fs::metadata(path)?;
        self.size = md.size();

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
#[repr(C)]
struct OptionReply {
    magic: [u8; 8],
    option: [u8; 4],
    reply_type: [u8; 4],
    length: [u8; 4],
}

// NBD client request
// #define NBD_REQUEST_SIZE            (4 + 2 + 2 + 8 + 8 + 4)
#[derive(Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
#[repr(C)]
struct Request {
    magic: [u8; 4],
    flags: [u8; 2],
    command_type: [u8; 2],
    handle: [u8; 8],
    offset: [u8; 8],
    len: [u8; 4],
}

impl<T> Server<T>
where
    T: Read + Write + Debug,
{
    pub fn new(export: Export) -> Self {
        let clients = Rc::new(RefCell::new(HashMap::new()));
        Server { clients, export }
    }

    pub fn handshake(&mut self, client: &str) -> Result<HandshakeResult, Box<dyn Error>> {
        self.export.init_flags()?;
        let mut clients = self.clients.borrow_mut();
        let c = clients.get_mut(client).unwrap();
        // 64 bits
        c.write_all(&NBD_INIT_MAGIC.to_be_bytes())?;

        // 64 bits
        c.write_all(&NBD_OPTS_MAGIC.to_be_bytes())?;

        // 16 bits
        let handshake_flags = NBD_FLAG_FIXED_NEWSTYLE | NBD_FLAG_NO_ZEROES;

        c.write_u16::<BigEndian>(handshake_flags)?;
        c.flush()?;

        // Start reading client negotiation
        // option flags
        let client_flags = c.read_u32::<BigEndian>()?;
        println!("Received client flags: {:#02x}", client_flags);
        if client_flags != NBD_FLAG_C_FIXED_NEWSTYLE
            && client_flags != (NBD_FLAG_C_FIXED_NEWSTYLE | NBD_FLAG_C_NO_ZEROES)
        {
            eprintln!("Unknown client flags {:#02x}", client_flags);
        }

        loop {
            // Check client magic
            let client_magic = c.read_u64::<BigEndian>()?;
            println!("Checking opts magic: {:#02x}", client_magic);
            if client_magic != NBD_OPTS_MAGIC {
                eprintln!(
                    "Bad magic received {:#02x}, expected {:#02x}",
                    client_magic, NBD_OPTS_MAGIC
                );
                continue;
            }

            // Read option
            let option = c.read_u32::<BigEndian>()?;
            println!("Checking option {:#02x}", option);

            // Read option length
            let option_length = c.read_u32::<BigEndian>()?;
            println!("Received option length {}", option_length);

            // TODO: Remove later
            let option: NbdOpt = unsafe { transmute(option) };

            match option {
                NbdOpt::Export => {
                    Self::handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::ExportName => {
                    Self::handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::List => {
                    Self::handle_list(c, &self.export.name, &self.export.description)?;
                }
                NbdOpt::Abort => {
                    println!("Aborting");
                    if Self::handshake_reply(c, option, NbdReply::Ack, EMPTY_REPLY).is_err() {
                        eprintln!("Ignoring abort ACK errors");
                    }
                    return Ok(HandshakeResult::Abort);
                }
                NbdOpt::StructuredReply => {
                    Self::handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                opt @ NbdOpt::Info => {
                    println!("received info!");
                    self.handle_export_info(c, opt)?;
                }
                opt @ NbdOpt::Go => {
                    println!("received go!");
                    self.handle_export_info(c, opt)?;
                    return Ok(HandshakeResult::Continue);
                }
                NbdOpt::ListMetaContext => {
                    Self::handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::SetMetaContext => {
                    Self::handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::StartTls => {
                    Self::handshake_reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
            }
        }
    }

    pub fn transmission(&self, client: &str) -> Result<(), Box<dyn Error>> {
        let mut clients = self.clients.borrow_mut();
        let c = clients.get_mut(client).unwrap();

        let f = File::open(&self.export.path)?;
        let mut request_buf: [u8; NBD_REQUEST_SIZE as usize] = [0; NBD_REQUEST_SIZE as usize];
        loop {
            c.read_exact(&mut request_buf)?;

            let request: Request = bincode::decode_from_slice(
                &request_buf,
                Configuration::standard()
                    .with_big_endian()
                    .with_variable_int_encoding(),
            )?;
            println!("request {:?}", request);

            println!("Checking opts magic: {:?}", request.magic);
            if u32::from_be_bytes(request.magic) != NBD_REQUEST_MAGIC {
                eprintln!(
                    "Bad magic received {:#02x}, expected {:#02x}",
                    u32::from_be_bytes(request.magic),
                    NBD_REQUEST_MAGIC
                );
                return Ok(());
            }

            let cmd: NbdCmd = unsafe { transmute(u16::from_be_bytes(request.command_type)) };
            match cmd {
                NbdCmd::Read => {
                    println!("Received read request");
                    let offset = u64::from_be_bytes(request.offset);
                    let len = u32::from_be_bytes(request.len);
                    let mut buf: Vec<u8> = vec![0; len as usize];
                    let read = f.read_at(buf.as_mut_slice(), offset)?;
                    println!("Read {} bytes", read);
                    Self::transmission_simple_reply_header(
                        c,
                        u64::from_be_bytes(request.handle),
                        0,
                    )?;
                    c.write_u64::<BigEndian>(read as u64)?;
                    c.flush()?;
                }
                NbdCmd::Write => {
                    println!("write!");
                }
                NbdCmd::Disc => {
                    println!("disconnect!");
                }
                NbdCmd::Flush => {
                    c.flush()?;
                    println!("flush!");
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

    pub fn add_connection(&mut self, client_addr: String, stream: T) -> Result<(), Box<dyn Error>> {
        self.clients.borrow_mut().insert(client_addr, stream);

        Ok(())
    }

    fn handle_export_info(&self, client: &mut T, opt: NbdOpt) -> Result<(), Box<dyn Error>> {
        // Read name length
        let len = client.read_u32::<BigEndian>()?;
        println!("Received length {}", len);
        let mut buf: Vec<u8> = vec![0; len as usize];

        // Read name
        client.read_exact(buf.as_mut_slice())?;
        let export_name = String::from_utf8(buf.clone())?;
        println!("Received export name {}", export_name);

        // Read number of requests
        let requests = client.read_u16::<BigEndian>()?;
        println!("Receiving {} request(s)", requests);

        let mut send_name = false;
        let mut send_description = false;
        let mut send_block_size = false;

        for i in 0..requests {
            // TODO use proper safe conversion
            let option = unsafe { transmute(client.read_u16::<BigEndian>()?) };
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
                    send_block_size = true;
                }
                NbdInfoOpt::Unknown => {
                    panic!("Shouldn't happen");
                }
            }
        }

        if send_name {
            Self::info_reply(
                client,
                opt,
                NbdInfoOpt::Name,
                self.export.name.len() as u32,
                self.export.name.as_bytes(),
            )?;
        }

        if send_description {
            Self::info_reply(
                client,
                opt,
                NbdInfoOpt::Description,
                self.export.description.len() as u32,
                self.export.description.as_bytes(),
            )?;
        }

        if send_block_size {
            let sizes: Vec<u32> = vec![
                MIN_BLOCK_SIZE,
                PREFERRED_BLOCK_SIZE,
                std::cmp::min(self.export.size as u32, MAX_BLOCK_SIZE),
            ];

            println!("sending size {:?}", sizes);
            Self::info_reply(
                client,
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
            Self::info_reply(client, opt, NbdInfoOpt::Export, 12, EMPTY_REPLY)?;

            client.write_all(&self.export.size.to_be_bytes())?;
            client.write_all(&flags.to_be_bytes())?;
            client.flush()?;
        }

        Self::handshake_reply(client, opt, NbdReply::Ack, EMPTY_REPLY)?;

        Ok(())
    }

    // TODO: support multiple (and actual) exports
    fn handle_list(client: &mut T, name: &str, description: &str) -> Result<(), Box<dyn Error>> {
        let reply_header = OptionReply {
            magic: NBD_REP_MAGIC.to_be_bytes(),
            option: (NbdOpt::List as u32).to_be_bytes(),
            reply_type: (NbdReply::Server as u32).to_be_bytes(),

            // Why +4? size of the length field (32)
            length: (name.len() as u32 + description.len() as u32 + 4).to_be_bytes(),
        };

        Self::header_reply(client, reply_header)?;
        client.write_all(&(name.len() as u32).to_be_bytes())?;
        client.write_all(name.as_bytes())?;
        client.write_all(description.as_bytes())?;
        client.flush()?;

        Self::handshake_reply(client, NbdOpt::List, NbdReply::Ack, EMPTY_REPLY)?;

        Ok(())
    }

    fn info_reply(
        client: &mut T,
        opt: NbdOpt,
        info_type: NbdInfoOpt,
        len: u32,
        data: &[u8],
    ) -> Result<(), Box<dyn Error>> {
        let header = OptionReply {
            magic: NBD_REP_MAGIC.to_be_bytes(),
            option: (opt as u32).to_be_bytes(),
            reply_type: (NbdReply::Info as u32).to_be_bytes(),
            length: len.to_be_bytes(),
        };

        client.write_all(&bincode::encode_to_vec(
            &header,
            Configuration::standard().with_big_endian(),
        )?)?;
        client.write_u16::<BigEndian>(info_type as u16)?;

        // Send payload
        if data != EMPTY_REPLY {
            dbg!(data);
            client.write_all(data)?;
        }
        client.flush()?;

        Ok(())
    }

    fn header_reply(client: &mut T, header: OptionReply) -> Result<(), Box<dyn Error>> {
        let config = Configuration::standard();
        config.with_big_endian();
        config.with_variable_int_encoding();
        let serialized = bincode::encode_to_vec(&header, config)?;
        dbg!(&serialized);
        client.write_all(&serialized)?;
        client.flush()?;

        Ok(())
    }

    fn handshake_reply(
        client: &mut T,
        client_option: NbdOpt,
        reply_type: NbdReply,
        data: &[u8],
    ) -> Result<(), Box<dyn Error>> {
        client.write_u64::<BigEndian>(NBD_REP_MAGIC)?;
        client.write_u32::<BigEndian>(client_option as u32)?;
        client.write_u32::<BigEndian>(reply_type as u32)?;
        client.write_u32::<BigEndian>(data.len() as u32)?;
        client.write_all(data)?;
        client.flush()?;

        Ok(())
    }

    fn transmission_simple_reply_header(
        client: &mut T,
        handle: u64,
        error: u32,
    ) -> Result<(), Box<dyn Error>> {
        client.write_u32::<BigEndian>(NBD_SIMPLE_REPLY_MAGIC)?;
        client.write_u32::<BigEndian>(error)?;
        client.write_u64::<BigEndian>(handle)?;
        client.flush()?;

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
}
