use std::{
    cell::RefCell,
    io::{Read, Write},
};

#[derive(Debug, Default)]
pub struct Client<T: Read + Write> {
    stream: RefCell<T>,
    structured_reply: bool,
    addr: String,
}

impl<T: Read + Write> Client<T> {
    pub fn new(stream: T, addr: String) -> Self {
        Client {
            stream: RefCell::new(stream),
            structured_reply: false,
            addr,
        }
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    pub fn stream(&self) -> &RefCell<T> {
        &self.stream
    }
}

impl<T: Read + Write> Write for Client<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.stream.borrow_mut().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.borrow_mut().flush()
    }
}

impl<T: Read + Write> Read for Client<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stream.borrow_mut().read(buf)
    }
}
