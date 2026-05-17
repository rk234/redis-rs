use std::io::{self, Error, Read, Write};

use mio::{Interest, net::TcpStream};

pub struct Conn {
    pub stream: TcpStream,
    pub want_close: bool,
    pub incoming: Vec<u8>,
    pub outgoing: Vec<u8>,
    pub current_interest: Interest,
}

impl Conn {
    pub fn new(conn: TcpStream) -> Conn {
        Conn {
            stream: conn,
            want_close: false,
            incoming: vec![],
            outgoing: vec![],
            current_interest: Interest::READABLE,
        }
    }

    pub fn desired_interest(&self) -> Interest {
        if !self.outgoing.is_empty() {
            Interest::READABLE.add(Interest::WRITABLE)
        } else {
            Interest::READABLE
        }
    }

    pub fn on_readable(&mut self) {
        let mut buf = [0; 64 * 1024];
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    if self.outgoing.is_empty() {
                        self.want_close = true;
                    }
                    break;
                }
                Ok(n) => {
                    self.incoming.extend_from_slice(&buf[..n]);
                }
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        break;
                    }

                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return;
                }
            }
        }
    }

    pub fn on_writable(&mut self) {
        loop {
            match self.stream.write(&self.outgoing) {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    self.outgoing.drain(..n);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break, // wait for next writable
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return,
            }
        }
    }
}
