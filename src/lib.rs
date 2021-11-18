use consts::{
    NBD_FLAG_C_FIXED_NEWSTYLE, NBD_FLAG_C_NO_ZEROES, NBD_FLAG_FIXED_NEWSTYLE, NBD_FLAG_NO_ZEROES,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::rc::Rc;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::consts::NBD_OPTS_MAGIC;

pub mod consts;

#[derive(Debug)]
pub struct Server<T: Read + Write + Debug> {
    clients: Rc<RefCell<HashMap<String, T>>>,
}

impl<T: Read + Write + Debug> Server<T> {
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
                panic!("Bad magic received");
            }

            // Read option
            let option = c.read_u32::<BigEndian>()?;
            println!("Checking option {:#02x}", option);

            // Read option length
            let option_length = c.read_u32::<BigEndian>()?;
            println!("Received option length {:#02x}", option_length);
        }

        Ok(())
    }

    pub fn add_connection(&mut self, client_addr: String, stream: T) -> Result<(), Box<dyn Error>> {
        self.clients.borrow_mut().insert(client_addr, stream);

        Ok(())
    }
}
