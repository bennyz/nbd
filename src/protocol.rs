use anyhow::Result;
use bincode::config::Configuration;
use byteorder::{BigEndian, WriteBytesExt};
use std::{
    fs::File,
    io::{Read, Write},
    os::unix::prelude::FileExt,
};

use crate::{
    client::Client,
    consts::{NbdInfoOpt, NbdOpt, NbdReply, NBD_REP_MAGIC, NBD_SIMPLE_REPLY_MAGIC},
};

pub const EMPTY_REPLY: &[u8; 0] = b"";

#[derive(Debug, bincode::Encode, bincode::Decode)]
#[repr(C)]
pub struct OptionReply {
    pub magic: u64,
    pub option: u32,
    pub reply_type: u32,
    pub length: u32,
}

#[derive(Debug, bincode::Encode, bincode::Decode)]
#[repr(C)]
pub struct StructuredReplyHeader {
    pub magic: u64,
    pub option: u32,
    pub reply_type: u32,
    pub length: u32,
}

// NBD client request
// #define NBD_REQUEST_SIZE            (4 + 2 + 2 + 8 + 8 + 4)
#[derive(Debug, bincode::Encode, bincode::Decode)]
#[repr(C)]
pub struct Request {
    pub magic: u32,
    pub flags: u16,
    pub command_type: u16,
    pub handle: u64,
    pub offset: u64,
    pub len: u32,
}

// TODO: support multiple (and actual) exports
pub fn handle_list<T: Read + Write>(
    c: &mut Client<T>,
    name: &str,
    description: &str,
) -> Result<()> {
    let reply_header = OptionReply {
        magic: NBD_REP_MAGIC,
        option: (NbdOpt::List as u32),
        reply_type: (NbdReply::Server as u32),

        // Why +4? size of the length field (32)
        length: (name.len() as u32 + description.len() as u32 + 4),
    };

    header_reply(c, reply_header)?;
    c.stream().write_all(&(name.len() as u32).to_be_bytes())?;
    c.stream().write_all(name.as_bytes())?;
    c.stream().write_all(description.as_bytes())?;
    c.stream().flush()?;

    handshake_reply(c, NbdOpt::List, NbdReply::Ack, EMPTY_REPLY)?;

    Ok(())
}

pub fn info_reply<T: Read + Write>(
    c: &mut Client<T>,
    opt: NbdOpt,
    info_type: NbdInfoOpt,
    len: u32,
    data: &[u8],
) -> Result<()> {
    let header = OptionReply {
        magic: NBD_REP_MAGIC,
        option: (opt as u32),
        reply_type: (NbdReply::Info as u32),
        length: len,
    };

    c.stream().write_all(&bincode::encode_to_vec(
        &header,
        Configuration::standard()
            .with_big_endian()
            .with_fixed_int_encoding(),
    )?)?;
    c.stream().write_u16::<BigEndian>(info_type as u16)?;

    // Send payload
    if data != EMPTY_REPLY {
        c.stream().write_all(data)?;
    }
    c.stream().flush()?;

    Ok(())
}

pub fn header_reply<T: Read + Write>(c: &mut Client<T>, header: OptionReply) -> Result<()> {
    let serialized = bincode::encode_to_vec(
        &header,
        Configuration::standard()
            .with_big_endian()
            .with_fixed_int_encoding(),
    )?;

    c.stream().write_all(&serialized)?;
    c.stream().flush()?;

    Ok(())
}

pub fn handshake_reply<T: Read + Write>(
    c: &mut Client<T>,
    client_option: NbdOpt,
    reply_type: NbdReply,
    data: &[u8],
) -> Result<()> {
    c.stream().write_u64::<BigEndian>(NBD_REP_MAGIC)?;
    c.stream().write_u32::<BigEndian>(client_option as u32)?;
    c.stream().write_u32::<BigEndian>(reply_type as u32)?;
    c.stream().write_u32::<BigEndian>(data.len() as u32)?;
    c.stream().write_all(data)?;
    c.stream().flush()?;

    Ok(())
}

pub fn transmission_simple_reply_header<T: Read + Write>(
    c: &mut Client<T>,
    handle: u64,
    error: u32,
) -> Result<()> {
    c.stream().write_u32::<BigEndian>(NBD_SIMPLE_REPLY_MAGIC)?;
    c.stream().write_u32::<BigEndian>(error)?;
    c.stream().write_u64::<BigEndian>(handle)?;

    Ok(())
}

pub fn do_read<T: Read + Write>(
    c: &mut Client<T>,
    handle: u64,
    offset: u64,
    len: u32,
    file: &File,
) -> Result<()> {
    let mut buf: Vec<u8> = vec![0; len as usize];
    let read = file.read_at(buf.as_mut_slice(), offset)?;
    println!("Read {} bytes", read);
    transmission_simple_reply_header(c, handle, 0)?;

    c.stream().write_all(&buf)?;
    c.stream().flush()?;
    Ok(())
}

pub fn do_write<T: Read + Write>(
    c: &mut Client<T>,
    handle: u64,
    offset: u64,
    len: u32,
    file: &File,
) -> Result<()> {
    let mut buf: Vec<u8> = vec![0; len as usize];
    c.stream().read_exact(buf.as_mut_slice())?;
    file.write_at(&buf, offset)?;
    transmission_simple_reply_header(c, handle, 0)?;
    c.stream().flush()?;

    Ok(())
}