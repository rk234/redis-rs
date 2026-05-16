use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const LISTENER: Token = Token(0);

fn main() -> io::Result<()> {
    println!("Logs from your program will appear here!");

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6379);
    let mut listener = TcpListener::bind(addr).unwrap();

    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(128);
    poll.registry()
        .register(&mut listener, LISTENER, Interest::READABLE);

    let mut connections: HashMap<Token, TcpStream> = HashMap::new();
    let mut next_token = Token(LISTENER.0 + 1);

    loop {
        if let Err(err) = poll.poll(&mut events, None) {
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }

        for event in events.iter() {
            match event.token() {
                LISTENER => loop {
                    let (mut conn, addr) = match listener.accept() {
                        Ok((conn, addr)) => (conn, addr),
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            break;
                        }
                        Err(e) => return Err(e),
                    };

                    println!("accepted new connection!");

                    let token = Token(next_token.0);
                    next_token.0 += 1;
                    poll.registry().register(
                        &mut conn,
                        token,
                        Interest::READABLE.add(Interest::WRITABLE),
                    );
                    connections.insert(token, conn);
                },
                token => {
                    let done = if let Some(mut conn) = connections.get_mut(&token) {
                        // if event.is_writable()
                        true
                    } else {
                        false
                    };
                }
            }
        }
    }
    for stream in listener.incoming() {
        match stream {
            Ok(mut _stream) => {
                println!("accepted new connection");
                let reader = BufReader::new(&_stream);
                let mut writer = BufWriter::new(&_stream);

                for _ in reader.lines() {
                    writer.write_all(b"+PONG\r\n");
                    writer.flush();
                }
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }

    Ok(())
}
