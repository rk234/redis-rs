use std::io;

mod command;
mod connection;
mod resp;
mod server;

fn main() -> io::Result<()> {
    println!("redis-rs running!");

    server::run("127.0.0.1:6379".parse().unwrap())
}
