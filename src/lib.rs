use consts::{
    NbdReply, NBD_FLAG_C_FIXED_NEWSTYLE, NBD_FLAG_C_NO_ZEROES, NBD_FLAG_FIXED_NEWSTYLE,
    NBD_FLAG_NO_ZEROES, NBD_REP_MAGIC,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{write, Debug};
use std::intrinsics::transmute;
use std::io::{Read, Write};
use std::rc::Rc;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::consts::{NbdOpt, NBD_OPTS_MAGIC};

pub mod consts;

const EMPTY_REPLY: &[u8; 0] = b"";

#[derive(Debug)]
pub struct Server<T: Read + Write + Debug> {
    clients: Rc<RefCell<HashMap<String, T>>>,
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
        c.write_all(b"NBDMAGIC")?;

        // 64 bits
        c.write_all(b"IHAVEOPT")?;

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
                    let name = b"\x00\x00\x00\x07name123".to_vec();
                    let description = b"description".to_vec();
                    let payload = [name, description].concat();

                    reply(c, option, NbdReply::Server, &payload)?;
                    reply(c, option, NbdReply::Ack, EMPTY_REPLY)?;
                }
                NbdOpt::Abort => {}
                _ => {
                    println!("Aborting");
                    reply(c, option, NbdReply::Ack, EMPTY_REPLY)?;
                }
            }
        }
    }

    pub fn add_connection(&mut self, client_addr: String, stream: T) -> Result<(), Box<dyn Error>> {
        self.clients.borrow_mut().insert(client_addr, stream);

        Ok(())
    }
}

fn reply<T: Read + Write + Debug>(
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
    println!("reply: {:?}, len {}", data, data.len());
    Ok(())
}
