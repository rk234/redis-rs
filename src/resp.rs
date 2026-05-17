#[derive(Debug, PartialEq)]
pub enum ParseResult {
    Complete(Vec<Vec<u8>>, usize), //args, # of bytes consumed,
    Malformed,
    Incomplete,
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

pub fn parse_one(buf: &[u8]) -> ParseResult {
    if buf.is_empty() {
        return ParseResult::Incomplete;
    }

    if buf[0] != b'*' {
        return ParseResult::Malformed;
    }

    match find_crlf(buf, 1) {
        Some(i) => {
            let mut cursor: usize = i;
            if let Some(n) = parse_int(&buf[1..cursor]) {
                cursor = i + 2;
                let mut args = vec![];
                for _ in 0..n {
                    if cursor >= buf.len() {
                        return ParseResult::Incomplete;
                    }
                    if buf[cursor] != b'$' {
                        return ParseResult::Malformed;
                    }
                    cursor += 1;

                    if let Some(crlf) = find_crlf(buf, cursor) {
                        if let Some(l) = parse_int(&buf[cursor..crlf]) {
                            if l < 0 {
                                return ParseResult::Malformed;
                            }

                            let payload_start = crlf + 2;
                            let payload_end = payload_start + l as usize;
                            if buf.len() < payload_end + 2 {
                                return ParseResult::Incomplete;
                            }
                            let arg = buf[payload_start..payload_end].to_vec();
                            cursor = payload_end + 2;
                            args.push(arg);
                        } else {
                            return ParseResult::Malformed;
                        }
                    } else {
                        return ParseResult::Malformed;
                    }
                }
                ParseResult::Complete(args, cursor)
            } else {
                ParseResult::Malformed
            }
        }
        None => ParseResult::Incomplete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ping() {
        assert_eq!(
            parse_one(b"*1\r\n$4\r\nping\r\n"),
            ParseResult::Complete(vec![b"ping".to_vec()], 14)
        );
    }

    #[test]
    fn parses_set_with_three_args() {
        assert_eq!(
            parse_one(b"*3\r\n$3\r\nSET\r\n$3\r\nfoo\r\n$3\r\nbar\r\n"),
            ParseResult::Complete(
                vec![b"SET".to_vec(), b"foo".to_vec(), b"bar".to_vec()],
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
            ParseResult::Complete(vec![b"ping".to_vec()], 14)
        );
    }

    #[test]
    fn preserves_payload_bytes_with_embedded_crlf() {
        // Bulk strings are byte-counted, not line-delimited — \r\n inside
        // the payload must NOT be treated as a terminator.
        let buf = b"*1\r\n$5\r\nhi\r\nx\r\n";
        assert_eq!(
            parse_one(buf),
            ParseResult::Complete(vec![b"hi\r\nx".to_vec()], 15)
        );
    }

    #[test]
    fn handles_zero_length_bulk_string() {
        assert_eq!(
            parse_one(b"*1\r\n$0\r\n\r\n"),
            ParseResult::Complete(vec![vec![]], 10)
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
    fn malformed_when_no_star_prefix() {
        assert_eq!(parse_one(b"ping\r\n"), ParseResult::Malformed);
    }

    #[test]
    fn malformed_when_arg_doesnt_start_with_dollar() {
        assert_eq!(parse_one(b"*1\r\n+ping\r\n"), ParseResult::Malformed);
    }

    #[test]
    fn malformed_when_array_count_not_an_integer() {
        assert_eq!(parse_one(b"*x\r\n"), ParseResult::Malformed);
    }

    #[test]
    fn malformed_on_negative_bulk_length() {
        // $-1\r\n is null-bulk — server→client only; clients shouldn't send it.
        assert_eq!(parse_one(b"*1\r\n$-1\r\n"), ParseResult::Malformed);
    }

    #[test]
    fn malformed_when_bulk_length_not_an_integer() {
        assert_eq!(parse_one(b"*1\r\n$abc\r\n"), ParseResult::Malformed);
    }
}
