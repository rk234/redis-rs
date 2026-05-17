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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_returns_pong() {
        let mut out = Vec::new();
        dispatch(vec![b"PING".to_vec()], &mut out);
        assert_eq!(out, b"+PONG\r\n");
    }

    #[test]
    fn ping_is_case_insensitive() {
        for variant in [b"ping" as &[u8], b"Ping", b"PiNg", b"PING"] {
            let mut out = Vec::new();
            dispatch(vec![variant.to_vec()], &mut out);
            assert_eq!(out, b"+PONG\r\n", "variant {:?}", variant);
        }
    }

    #[test]
    fn unknown_command_returns_error() {
        let mut out = Vec::new();
        dispatch(vec![b"NOPE".to_vec()], &mut out);
        assert_eq!(out, b"-ERR unknown command\r\n");
    }

    #[test]
    fn empty_args_returns_error() {
        let mut out = Vec::new();
        dispatch(vec![], &mut out);
        assert_eq!(out, b"-ERR unknown command\r\n");
    }

    #[test]
    fn appends_to_existing_outgoing_buffer() {
        // dispatch must not clobber any reply already queued in the buffer.
        let mut out = b"+PREVIOUS\r\n".to_vec();
        dispatch(vec![b"PING".to_vec()], &mut out);
        assert_eq!(out, b"+PREVIOUS\r\n+PONG\r\n");
    }
}
