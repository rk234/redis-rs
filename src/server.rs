use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::SocketAddr,
};

use mio::{Events, Interest, Poll, Token, net::TcpListener};

use crate::{
    command::dispatch,
    connection::Conn,
    resp::{self, ParseResult, parse_one},
};

const LISTENER: Token = Token(0);

pub fn run(addr: SocketAddr) -> io::Result<()> {
    let mut listener = TcpListener::bind(addr).unwrap();

    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(128);
    poll.registry()
        .register(&mut listener, LISTENER, Interest::READABLE)?;

    let mut connections: HashMap<Token, Conn> = HashMap::new();
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
                    let (mut conn, _addr) = match listener.accept() {
                        Ok((conn, addr)) => (conn, addr),
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            break;
                        }
                        Err(e) => return Err(e),
                    };

                    println!("accepted new connection!");

                    let token = Token(next_token.0);
                    next_token.0 += 1;
                    poll.registry()
                        .register(&mut conn, token, Interest::READABLE)?;
                    connections.insert(token, Conn::new(conn));
                },
                token => {
                    if let Some(conn) = connections.get_mut(&token) {
                        if event.is_readable() {
                            conn.on_readable();
                            println!(
                                "incoming: |{:?}|",
                                std::str::from_utf8(&conn.incoming).unwrap()
                            );
                        }

                        while !conn.incoming.is_empty() {
                            match parse_one(&conn.incoming) {
                                ParseResult::Complete(args, n) => {
                                    println!(
                                        "parsed: |{:?}|",
                                        std::str::from_utf8(&conn.incoming[..n]).unwrap()
                                    );
                                    conn.incoming.drain(..n);
                                    dispatch(args, &mut conn.outgoing);
                                }
                                ParseResult::Malformed => {
                                    conn.want_close = true;
                                    break;
                                }
                                ParseResult::Incomplete => {
                                    break;
                                }
                            }
                        }

                        if conn.current_interest != conn.desired_interest() {
                            conn.current_interest = conn.desired_interest();
                            poll.registry().reregister(
                                &mut conn.stream,
                                token,
                                conn.current_interest,
                            );
                        }

                        if !conn.outgoing.is_empty() {
                            conn.on_writable();
                        }

                        if conn.want_close {
                            poll.registry().deregister(&mut conn.stream)?;
                            connections.remove(&token);
                        }
                    };
                }
            }
        }
    }
}
