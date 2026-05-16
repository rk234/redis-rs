#![allow(unused_imports)]
use std::{
    io::{BufRead, BufReader, BufWriter, Read, Write},
    net::TcpListener,
};

fn main() {
    println!("Logs from your program will appear here!");
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();

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
}
