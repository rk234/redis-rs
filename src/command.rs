pub fn dispatch(args: Vec<Vec<u8>>, out: &mut Vec<u8>) {
    match args.first().map(|a| a.as_slice()) {
        Some(cmd) if cmd.eq_ignore_ascii_case(b"PING") => {
            out.extend_from_slice(b"+PONG\r\n");
        }
        _ => {
            out.extend_from_slice(b"-ERR unknown command\r\n");
        }
    }
}
