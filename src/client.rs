use std::io::{Read, Write};

#[derive(Debug, Default)]
pub struct Client<T: Read + Write> {
    stream: T,
    structured_reply: bool,
    addr: String,
}

impl<T: Read + Write> Client<T> {
    pub fn new(stream: T, addr: String) -> Self {
        Client {
            stream,
            structured_reply: false,
            addr,
        }
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    pub fn stream(&mut self) -> &mut T {
        &mut self.stream
    }
}

impl<T: Read + Write> Write for Client<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

impl<T: Read + Write> Read for Client<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stream.read(buf)
    }
}

impl<T: Read + Write> Drop for Client<T> {
    fn drop(&mut self) {
        println!("Client {} disconnected", self.addr);
    }
}
