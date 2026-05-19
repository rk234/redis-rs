#[derive(Debug, PartialEq)]
pub enum ParseResult {
    Complete(Resp, usize), //args, # of bytes consumed,
    Malformed,
    Incomplete,
}

#[derive(Debug, PartialEq)]
pub enum Resp {
    String(String),
    Error(String),
    Integer(i64),
    BulkString(i64, String), //len, string
    NullBulkString,
    Array(i64, Vec<Resp>),
    Boolean(bool),
    Double(f64),
}

fn find_crlf(buf: &[u8], from: usize) -> Option<usize> {
    buf[from..]
        .windows(2)
        .position(|w| w == b"\r\n")
        .map(|p| from + p)
}

fn parse_int(buf: &[u8]) -> Option<i64> {
    std::str::from_utf8(buf).ok()?.parse().ok()
}

fn parse_f64(buf: &[u8]) -> Option<f64> {
    std::str::from_utf8(buf).ok()?.parse().ok()
}

fn parse_simple_string(buf: &[u8]) -> ParseResult {
    match find_crlf(buf, 1) {
        Some(i) => match String::from_utf8(buf[1..i].to_vec()) {
            Ok(s) => ParseResult::Complete(Resp::String(s), i + 2),
            Err(_) => ParseResult::Malformed,
        },
        None => ParseResult::Incomplete,
    }
}

fn parse_error(buf: &[u8]) -> ParseResult {
    match find_crlf(buf, 1) {
        Some(i) => match String::from_utf8(buf[1..i].to_vec()) {
            Ok(s) => ParseResult::Complete(Resp::Error(s), i + 2),
            Err(_) => ParseResult::Malformed,
        },
        None => ParseResult::Incomplete,
    }
}

fn parse_integer(buf: &[u8]) -> ParseResult {
    match find_crlf(buf, 1) {
        Some(i) => match parse_int(&buf[1..i]) {
            Some(n) => ParseResult::Complete(Resp::Integer(n), i + 2),
            None => ParseResult::Malformed,
        },
        None => ParseResult::Incomplete,
    }
}

fn parse_bulk_string(buf: &[u8]) -> ParseResult {
    let Some(i) = find_crlf(buf, 1) else {
        return ParseResult::Incomplete;
    };
    let Some(len) = parse_int(&buf[1..i]) else {
        return ParseResult::Malformed;
    };
    if len < -1 {
        return ParseResult::Malformed;
    }
    if len == -1 {
        return ParseResult::Complete(Resp::NullBulkString, i + 2);
    }
    let string_start = i + 2;
    let string_end = string_start + len as usize;
    if string_end + 2 > buf.len() {
        return ParseResult::Incomplete;
    }
    match find_crlf(buf, string_end) {
        Some(i) => match String::from_utf8(buf[string_start..i].to_vec()) {
            Ok(s) => ParseResult::Complete(Resp::BulkString(len, s), i + 2),
            Err(_) => ParseResult::Malformed,
        },
        None => ParseResult::Incomplete,
    }
}

fn parse_boolean(buf: &[u8]) -> ParseResult {
    match find_crlf(buf, 1) {
        Some(i) => match buf[1..i] {
            [b't'] => ParseResult::Complete(Resp::Boolean(true), i + 2),
            [b'f'] => ParseResult::Complete(Resp::Boolean(false), i + 2),
            _ => ParseResult::Malformed,
        },
        None => ParseResult::Incomplete,
    }
}

fn parse_double(buf: &[u8]) -> ParseResult {
    match find_crlf(buf, 1) {
        Some(i) => match parse_f64(&buf[1..i]) {
            Some(d) => ParseResult::Complete(Resp::Double(d), i + 2),
            None => ParseResult::Malformed,
        },
        None => ParseResult::Incomplete,
    }
}

fn parse_array(buf: &[u8]) -> ParseResult {
    let Some(i) = find_crlf(buf, 1) else {
        return ParseResult::Incomplete;
    };
    let Some(len) = parse_int(&buf[1..i]) else {
        return ParseResult::Malformed;
    };
    let mut cursor = i + 2;
    let mut elements = Vec::with_capacity(len as usize);
    for _ in 0..len {
        match parse_resp_msg(&buf[cursor..]) {
            ParseResult::Incomplete => return ParseResult::Incomplete,
            ParseResult::Malformed => return ParseResult::Malformed,
            ParseResult::Complete(resp, end) => {
                cursor += end;
                elements.push(resp);
            }
        }
    }
    ParseResult::Complete(Resp::Array(len, elements), cursor)
}

pub fn parse_resp_msg(buf: &[u8]) -> ParseResult {
    if buf.is_empty() {
        return ParseResult::Incomplete;
    }
    match buf[0] {
        b'+' => parse_simple_string(buf),
        b'-' => parse_error(buf),
        b':' => parse_integer(buf),
        b'$' => parse_bulk_string(buf),
        b'#' => parse_boolean(buf),
        b',' => parse_double(buf),
        b'*' => parse_array(buf),
        _ => ParseResult::Malformed,
    }
}

pub fn parse_one(buf: &[u8]) -> ParseResult {
    parse_resp_msg(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_string() {
        assert_eq!(
            parse_one(b"+OK\r\n"),
            ParseResult::Complete(Resp::String("OK".to_string()), 5)
        );
    }

    #[test]
    fn parses_simple_error() {
        assert_eq!(
            parse_one(b"-ERR something went wrong\r\n"),
            ParseResult::Complete(Resp::Error("ERR something went wrong".to_string()), 27)
        );
    }

    #[test]
    fn parses_integer() {
        assert_eq!(
            parse_one(b":42\r\n"),
            ParseResult::Complete(Resp::Integer(42), 5)
        );
    }

    #[test]
    fn parses_negative_integer() {
        assert_eq!(
            parse_one(b":-123\r\n"),
            ParseResult::Complete(Resp::Integer(-123), 7)
        );
    }

    #[test]
    fn parses_boolean_true() {
        assert_eq!(
            parse_one(b"#t\r\n"),
            ParseResult::Complete(Resp::Boolean(true), 4)
        );
    }

    #[test]
    fn parses_boolean_false() {
        assert_eq!(
            parse_one(b"#f\r\n"),
            ParseResult::Complete(Resp::Boolean(false), 4)
        );
    }

    #[test]
    fn malformed_when_boolean_not_t_or_f() {
        assert_eq!(parse_one(b"#x\r\n"), ParseResult::Malformed);
    }

    #[test]
    fn parses_double() {
        assert_eq!(
            parse_one(b",3.14\r\n"),
            ParseResult::Complete(Resp::Double(3.14), 7)
        );
    }

    #[test]
    fn malformed_when_double_not_a_number() {
        assert_eq!(parse_one(b",abc\r\n"), ParseResult::Malformed);
    }

    #[test]
    fn parses_ping() {
        assert_eq!(
            parse_one(b"*1\r\n$4\r\nping\r\n"),
            ParseResult::Complete(
                Resp::Array(1, vec![Resp::BulkString(4, "ping".to_string())]),
                14
            )
        );
    }

    #[test]
    fn parses_set_with_three_args() {
        assert_eq!(
            parse_one(b"*3\r\n$3\r\nSET\r\n$3\r\nfoo\r\n$3\r\nbar\r\n"),
            ParseResult::Complete(
                Resp::Array(
                    3,
                    vec![
                        Resp::BulkString(3, "SET".to_string()),
                        Resp::BulkString(3, "foo".to_string()),
                        Resp::BulkString(3, "bar".to_string()),
                    ]
                ),
                31
            )
        );
    }

    #[test]
    fn returns_only_first_frame_when_pipelined() {
        // Two PINGs back-to-back: parser returns the first and reports
        // exactly its byte length so the caller can drain and parse again.
        let buf = b"*1\r\n$4\r\nping\r\n*1\r\n$4\r\nping\r\n";
        assert_eq!(
            parse_one(buf),
            ParseResult::Complete(
                Resp::Array(1, vec![Resp::BulkString(4, "ping".to_string())]),
                14
            )
        );
    }

    #[test]
    fn preserves_payload_bytes_with_embedded_crlf() {
        // Bulk strings are byte-counted, not line-delimited — \r\n inside
        // the payload must NOT be treated as a terminator.
        let buf = b"*1\r\n$5\r\nhi\r\nx\r\n";
        assert_eq!(
            parse_one(buf),
            ParseResult::Complete(
                Resp::Array(1, vec![Resp::BulkString(5, "hi\r\nx".to_string())]),
                15
            )
        );
    }

    #[test]
    fn handles_zero_length_bulk_string() {
        assert_eq!(
            parse_one(b"*1\r\n$0\r\n\r\n"),
            ParseResult::Complete(
                Resp::Array(1, vec![Resp::BulkString(0, "".to_string())]),
                10
            )
        );
    }

    #[test]
    fn empty_buffer_is_incomplete() {
        assert_eq!(parse_one(b""), ParseResult::Incomplete);
    }

    #[test]
    fn incomplete_when_header_missing_crlf() {
        assert_eq!(parse_one(b"*1"), ParseResult::Incomplete);
    }

    #[test]
    fn incomplete_when_array_header_present_but_no_args_yet() {
        assert_eq!(parse_one(b"*1\r\n"), ParseResult::Incomplete);
    }

    #[test]
    fn incomplete_when_bulk_length_present_but_payload_truncated() {
        assert_eq!(parse_one(b"*1\r\n$4\r\npin"), ParseResult::Incomplete);
    }

    #[test]
    fn incomplete_when_payload_present_but_trailing_crlf_truncated() {
        // 13 bytes — payload complete, only \r of trailing \r\n arrived.
        assert_eq!(parse_one(b"*1\r\n$4\r\nping\r"), ParseResult::Incomplete);
    }

    #[test]
    fn malformed_when_unknown_prefix() {
        assert_eq!(parse_one(b"ping\r\n"), ParseResult::Malformed);
    }

    #[test]
    fn malformed_when_array_element_has_unknown_prefix() {
        assert_eq!(parse_one(b"*1\r\nzping\r\n"), ParseResult::Malformed);
    }

    #[test]
    fn malformed_when_array_count_not_an_integer() {
        assert_eq!(parse_one(b"*x\r\n"), ParseResult::Malformed);
    }

    #[test]
    fn null_bulk_string() {
        assert_eq!(
            parse_one(b"$-1\r\n"),
            ParseResult::Complete(Resp::NullBulkString, 5)
        );
    }

    #[test]
    fn null_bulk_string_inside_array() {
        assert_eq!(
            parse_one(b"*1\r\n$-1\r\n"),
            ParseResult::Complete(Resp::Array(1, vec![Resp::NullBulkString]), 9)
        );
    }

    #[test]
    fn malformed_when_bulk_length_below_minus_one() {
        assert_eq!(parse_one(b"$-2\r\n"), ParseResult::Malformed);
    }

    #[test]
    fn malformed_when_bulk_length_not_an_integer() {
        assert_eq!(parse_one(b"*1\r\n$abc\r\n"), ParseResult::Malformed);
    }
}
