//! Lexer for the IRQ 0.1 starter syntax.

use irq_diagnostics::codes;

pub const MAX_SOURCE_BYTES: usize = 1_048_576;
pub const MAX_TOKENS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// UTF-8 byte offset, inclusive.
    pub start: usize,
    /// UTF-8 byte offset, exclusive.
    pub end: usize,
    /// One-based source line.
    pub line: usize,
    /// One-based Unicode-scalar column; tabs currently count as one column.
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String),
    Number(String),
    String(String),
    Newline,
    Colon,
    Comma,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Arrow,
    Eq,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Pipe,
    Amp,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(LexError {
            code: codes::SOURCE_TOO_LARGE,
            message: format!("source exceeds the {MAX_SOURCE_BYTES} byte limit"),
            span: Span {
                start: 0,
                end: 0,
                line: 1,
                column: 1,
            },
        });
    }
    let chars: Vec<char> = source.chars().collect();
    let byte_offsets: Vec<usize> = source
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(source.len()))
        .collect();
    let (mut i, mut line, mut col) = (0usize, 1usize, 1usize);
    let mut line_has_code = false;
    let mut out = Vec::new();
    while i < chars.len() {
        let start = i;
        let (start_line, start_col) = (line, col);
        let c = chars[i];
        if c == ' ' || c == '\t' || c == '\r' {
            i += 1;
            col += 1;
            continue;
        }
        if c == '#' {
            if line_has_code {
                return Err(LexError {
                    code: codes::LEX_ERROR,
                    message: "comments must occupy a line by themselves".into(),
                    span: Span {
                        start: byte_offsets[start],
                        end: byte_offsets[start + 1],
                        line,
                        column: col,
                    },
                });
            }
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        }
        if c == '\n' {
            if out.len() >= MAX_TOKENS {
                return Err(token_limit_error(line, col));
            }
            i += 1;
            line += 1;
            col = 1;
            line_has_code = false;
            out.push(Token {
                kind: TokenKind::Newline,
                span: Span {
                    start: byte_offsets[start],
                    end: byte_offsets[i],
                    line: start_line,
                    column: start_col,
                },
            });
            continue;
        }
        let kind = if c.is_ascii_alphabetic() || c == '_' {
            i += 1;
            col += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
                col += 1;
            }
            TokenKind::Ident(chars[start..i].iter().collect())
        } else if c.is_ascii_digit() {
            i += 1;
            col += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
                col += 1;
            }
            TokenKind::Number(chars[start..i].iter().collect())
        } else if c == '"' {
            i += 1;
            col += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < chars.len() {
                match chars[i] {
                    '"' => {
                        i += 1;
                        col += 1;
                        closed = true;
                        break;
                    }
                    '\\' if i + 1 < chars.len() => {
                        let escape_start = i;
                        i += 1;
                        col += 1;
                        let unescaped = match chars[i] {
                            'n' => '\n',
                            't' => '\t',
                            'r' => '\r',
                            '"' => '"',
                            '\\' => '\\',
                            escaped => {
                                return Err(LexError {
                                    code: codes::LEX_ERROR,
                                    message: format!("unsupported escape sequence: \\{escaped}"),
                                    span: Span {
                                        start: byte_offsets[escape_start],
                                        end: byte_offsets[i + 1],
                                        line,
                                        column: col - 1,
                                    },
                                });
                            }
                        };
                        value.push(unescaped);
                        i += 1;
                        col += 1;
                    }
                    '\\' => {
                        return Err(LexError {
                            code: codes::LEX_ERROR,
                            message: "incomplete escape sequence at end of string".into(),
                            span: Span {
                                start: byte_offsets[i],
                                end: byte_offsets[i + 1],
                                line,
                                column: col,
                            },
                        });
                    }
                    '\n' => break,
                    ch => {
                        value.push(ch);
                        i += 1;
                        col += 1;
                    }
                }
            }
            if !closed {
                return Err(LexError {
                    code: codes::LEX_ERROR,
                    message: "unterminated string literal".into(),
                    span: Span {
                        start: byte_offsets[start],
                        end: byte_offsets[i],
                        line: start_line,
                        column: start_col,
                    },
                });
            }
            TokenKind::String(value)
        } else {
            let pair = if i + 1 < chars.len() {
                Some((c, chars[i + 1]))
            } else {
                None
            };
            let double = pair.and_then(|p| match p {
                ('-', '>') => Some(TokenKind::Arrow),
                ('=', '=') => Some(TokenKind::EqEq),
                ('!', '=') => Some(TokenKind::NotEq),
                ('<', '=') => Some(TokenKind::Le),
                ('>', '=') => Some(TokenKind::Ge),
                _ => None,
            });
            if let Some(t) = double {
                i += 2;
                col += 2;
                t
            } else {
                single(c, &mut i, &mut col).ok_or_else(|| LexError {
                    code: codes::LEX_ERROR,
                    message: format!("unexpected character: {c}"),
                    span: Span {
                        start: byte_offsets[start],
                        end: byte_offsets[start + 1],
                        line,
                        column: col,
                    },
                })?
            }
        };
        if out.len() >= MAX_TOKENS {
            return Err(token_limit_error(start_line, start_col));
        }
        out.push(Token {
            kind,
            span: Span {
                start: byte_offsets[start],
                end: byte_offsets[i],
                line: start_line,
                column: start_col,
            },
        });
        line_has_code = true;
    }
    out.push(Token {
        kind: TokenKind::Eof,
        span: Span {
            start: source.len(),
            end: source.len(),
            line,
            column: col,
        },
    });
    Ok(out)
}

fn token_limit_error(line: usize, column: usize) -> LexError {
    LexError {
        code: codes::TOKEN_LIMIT,
        message: format!("token count exceeds the {MAX_TOKENS} token limit"),
        span: Span {
            start: 0,
            end: 0,
            line,
            column,
        },
    }
}

fn single(c: char, i: &mut usize, col: &mut usize) -> Option<TokenKind> {
    let t = match c {
        ':' => TokenKind::Colon,
        ',' => TokenKind::Comma,
        '(' => TokenKind::LParen,
        ')' => TokenKind::RParen,
        '[' => TokenKind::LBracket,
        ']' => TokenKind::RBracket,
        '=' => TokenKind::Eq,
        '<' => TokenKind::Lt,
        '>' => TokenKind::Gt,
        '+' => TokenKind::Plus,
        '-' => TokenKind::Minus,
        '*' => TokenKind::Star,
        '/' => TokenKind::Slash,
        '%' => TokenKind::Percent,
        '|' => TokenKind::Pipe,
        '&' => TokenKind::Amp,
        _ => return None,
    };
    *i += 1;
    *col += 1;
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lexes_signature_and_skips_comment() {
        let ts = lex("# hi\nfn add(a: i32) -> i32\n").unwrap();
        assert_eq!(ts[0].kind, TokenKind::Newline);
        assert_eq!(ts[1].kind, TokenKind::Ident("fn".into()));
        assert!(ts.iter().any(|t| t.kind == TokenKind::Arrow));
        assert!(ts.iter().any(|t| t.kind == TokenKind::Newline));
    }

    #[test]
    fn rejects_inline_comment_after_code() {
        let error = lex("return 0 # not a comment line").unwrap_err();
        assert_eq!(error.code, codes::LEX_ERROR);
        assert!(error.message.contains("line by themselves"));
    }
    #[test]
    fn rejects_unterminated_string() {
        assert!(lex("\"oops").is_err());
    }

    #[test]
    fn spans_after_arabic_text_are_utf8_byte_offsets() {
        let source = "\"عربي\" + 1";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].span.start, 0);
        assert_eq!(tokens[0].span.end, "\"عربي\"".len());
        assert_eq!(tokens[1].span.start, source.find('+').unwrap());
        assert_eq!(tokens[2].span.start, source.find('1').unwrap());
    }

    #[test]
    fn lexical_error_after_arabic_comment_has_correct_byte_span() {
        let source = "# تعليق عربي\n@";
        let error = lex(source).unwrap_err();
        let expected = source.find('@').unwrap();
        assert_eq!(error.span.start, expected);
        assert_eq!(error.span.end, expected + 1);
        assert_eq!(error.span.line, 2);
        assert_eq!(error.span.column, 1);
    }

    #[test]
    fn rejects_unsupported_and_incomplete_escapes() {
        assert!(
            lex("\"\\q\"")
                .unwrap_err()
                .message
                .contains("unsupported escape sequence")
        );
        assert!(
            lex("\"abc\\")
                .unwrap_err()
                .message
                .contains("incomplete escape sequence")
        );
    }

    #[test]
    fn rejects_oversized_source_before_scanning() {
        let source = "x".repeat(MAX_SOURCE_BYTES + 1);
        let error = lex(&source).unwrap_err();
        assert_eq!(error.code, codes::SOURCE_TOO_LARGE);
        assert_eq!(error.span.start, 0);
    }

    #[test]
    fn rejects_excessive_token_count() {
        let source = "a ".repeat(MAX_TOKENS + 1);
        let error = lex(&source).unwrap_err();
        assert_eq!(error.code, codes::TOKEN_LIMIT);
    }

    #[test]
    fn deterministic_fuzz_smoke_never_panics() {
        let alphabet = [
            'a', '1', '_', ' ', '\n', '#', '"', '\\', 'ع', '(', '+', '-', '/', '\t',
        ];
        let mut state = 0x5eed_u64;
        for _ in 0..1_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let length = (state as usize) % 256;
            let mut input = String::with_capacity(length * 2);
            for _ in 0..length {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                input.push(alphabet[(state as usize) % alphabet.len()]);
            }
            assert!(std::panic::catch_unwind(|| lex(&input)).is_ok());
        }
    }
}
