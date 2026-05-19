use crate::resp::Resp;

pub fn dispatch(cmd: Resp, out: &mut Vec<u8>) {
    let cmd_name = match &cmd {
        Resp::Array(_, elements) => match elements.first() {
            Some(Resp::BulkString(_, s)) => Some(s.as_str()),
            _ => None,
        },
        _ => None,
    };

    match cmd_name {
        Some(name) if name.eq_ignore_ascii_case("PING") => {
            Resp::String(String::from("PONG")).serialize(out);
        }
        _ => {
            Resp::Error(String::from("ERR unknown command")).serialize(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bulk(s: &str) -> Resp {
        Resp::BulkString(s.len() as i64, s.to_string())
    }

    fn array(elements: Vec<Resp>) -> Resp {
        Resp::Array(elements.len() as i64, elements)
    }

    #[test]
    fn ping_returns_pong() {
        let mut out = Vec::new();
        dispatch(array(vec![bulk("PING")]), &mut out);
        assert_eq!(out, b"+PONG\r\n");
    }

    #[test]
    fn ping_is_case_insensitive() {
        for variant in ["ping", "Ping", "PiNg", "PING"] {
            let mut out = Vec::new();
            dispatch(array(vec![bulk(variant)]), &mut out);
            assert_eq!(out, b"+PONG\r\n", "variant {:?}", variant);
        }
    }

    #[test]
    fn unknown_command_returns_error() {
        let mut out = Vec::new();
        dispatch(array(vec![bulk("NOPE")]), &mut out);
        assert_eq!(out, b"-ERR unknown command\r\n");
    }

    #[test]
    fn empty_array_returns_error() {
        let mut out = Vec::new();
        dispatch(array(vec![]), &mut out);
        assert_eq!(out, b"-ERR unknown command\r\n");
    }

    #[test]
    fn non_array_top_level_returns_error() {
        // Clients send commands as arrays of bulk strings; anything else is bogus.
        let mut out = Vec::new();
        dispatch(Resp::String("PING".to_string()), &mut out);
        assert_eq!(out, b"-ERR unknown command\r\n");
    }

    #[test]
    fn non_bulk_first_element_returns_error() {
        let mut out = Vec::new();
        dispatch(array(vec![Resp::Integer(1)]), &mut out);
        assert_eq!(out, b"-ERR unknown command\r\n");
    }

    #[test]
    fn appends_to_existing_outgoing_buffer() {
        let mut out = b"+PREVIOUS\r\n".to_vec();
        dispatch(array(vec![bulk("PING")]), &mut out);
        assert_eq!(out, b"+PREVIOUS\r\n+PONG\r\n");
    }
}
