use consts::{
    NbdReply, NBD_FLAG_C_FIXED_NEWSTYLE, NBD_FLAG_C_NO_ZEROES, NBD_FLAG_FIXED_NEWSTYLE,
    NBD_FLAG_NO_ZEROES, NBD_REP_MAGIC,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::intrinsics::transmute;
use std::io::{Read, Write};
use std::rc::Rc;

use bincode::config::Configuration;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::consts::{NbdOpt, NBD_INIT_MAGIC, NBD_OPTS_MAGIC};

pub mod consts;

const EMPTY_REPLY: &[u8; 0] = b"";

#[derive(Debug)]
pub struct Server<T: Read + Write + Debug> {
    clients: Rc<RefCell<HashMap<String, T>>>,
}

#[derive(Debug, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
#[repr(C)]
struct OptionReply {
    magic: [u8; 8],
    option: [u8; 4],
    reply_type: [u8; 4],
    length: [u8; 4],
}

impl<T> Server<T>
where
    T: Read + Write + Debug,
{
    pub fn new() -> Self {
        let clients = Rc::new(RefCell::new(HashMap::new()));
        Server { clients }
    }

    pub fn negotiate(&mut self, client: &str) -> Result<(), Box<dyn Error>> {
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
                eprintln!("Bad magic received {:#02x}", client_magic);
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
                NbdOpt::ExportName => {
                    println!("export name!");
                }
                NbdOpt::List => {
                    Self::handle_list(c, "kawabanga", "babanaga")?;
                }
                NbdOpt::Abort => {
                    println!("Aborting");
                    Self::reply(c, option, NbdReply::Ack, EMPTY_REPLY)?;
                    break;
                }
                NbdOpt::StructuredReply => {
                    Self::reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::Info => {
                    Self::reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::Go => {
                    Self::reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::ListMetaContext => {
                    Self::reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::SetMetaContext => {
                    Self::reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
                NbdOpt::StartTls => {
                    Self::reply(c, option, NbdReply::NbdRepErrUnsup, EMPTY_REPLY)?;
                }
            }
        }

        Ok(())
    }

    pub fn add_connection(&mut self, client_addr: String, stream: T) -> Result<(), Box<dyn Error>> {
        self.clients.borrow_mut().insert(client_addr, stream);

        Ok(())
    }

    // TODO: support multiple (and actual) exports
    fn handle_list(client: &mut T, name: &str, description: &str) -> Result<(), Box<dyn Error>> {
        let reply_header = OptionReply {
            magic: NBD_REP_MAGIC.to_be_bytes(),
            option: (NbdOpt::List as u32).to_be_bytes(),
            reply_type: (NbdReply::Server as u32).to_be_bytes(),
            length: (name.len() as u32 + description.len() as u32 + 4).to_be_bytes(),
        };

        Self::header_reply(client, reply_header)?;
        client.write_all(&(name.len() as u32).to_be_bytes())?;
        client.write_all(name.as_bytes())?;
        client.write_all(description.as_bytes())?;
        client.flush()?;

        Self::reply(client, NbdOpt::List, NbdReply::Ack, EMPTY_REPLY)?;

        Ok(())
    }

    fn header_reply(client: &mut T, header: OptionReply) -> Result<(), Box<dyn Error>> {
        let config = Configuration::standard();
        config.with_big_endian();
        config.with_variable_int_encoding();
        let serialized = bincode::encode_to_vec(&header, config)?;
        client.write_all(&serialized)?;
        client.flush()?;

        Ok(())
    }

    fn reply(
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
}
