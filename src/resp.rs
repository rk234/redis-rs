use crate::connection::Conn;

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
        return ParseResult::Malformed;
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
                    if buf[cursor] != b'$' {
                        return ParseResult::Malformed;
                    }
                    cursor += 1;

                    if let Some(crlf) = find_crlf(buf, cursor) {
                        if let Some(l) = parse_int(&buf[cursor..crlf]) {
                            if ((crlf + l as usize) + 2) >= buf.len() {
                                return ParseResult::Incomplete;
                            }
                            let payload_start = crlf + 2;
                            let payload_end = payload_start + l as usize;
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
