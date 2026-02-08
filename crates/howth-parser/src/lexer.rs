//! Lexer (tokenizer) for JavaScript/TypeScript/JSX.
//!
//! The lexer converts source text into a stream of tokens.
//! It's called on-demand by the parser, not upfront, which enables
//! context-sensitive tokenization (e.g., regex vs division).

use crate::span::Span;
use crate::token::{Token, TokenKind, keyword_from_str};

/// The lexer state.
#[derive(Clone)]
pub struct Lexer<'a> {
    /// Source code as bytes (for fast indexing).
    source: &'a [u8],
    /// Current byte position.
    pos: usize,
    /// Start position of the current token.
    token_start: usize,
    /// Whether the previous token allows a regex to follow.
    /// This disambiguates `/regex/` vs `a / b`.
    allow_regex: bool,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given source code.
    pub fn new(source: &'a str) -> Self {
        Self {
            source: source.as_bytes(),
            pos: 0,
            token_start: 0,
            allow_regex: true, // At start of file, regex is allowed
        }
    }

    /// Get the current byte position.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Get the next token.
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();
        self.token_start = self.pos;

        if self.is_eof() {
            return self.make_token(TokenKind::Eof);
        }

        let ch = self.current();
        let kind = match ch {
            // Identifiers and keywords
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => self.scan_identifier(),

            // Numbers
            b'0'..=b'9' => self.scan_number(),

            // Strings
            b'"' | b'\'' => self.scan_string(ch),

            // Template literals
            b'`' => self.scan_template_head(),

            // Punctuation and operators
            b'(' => { self.advance(); TokenKind::LParen }
            b')' => { self.advance(); TokenKind::RParen }
            b'{' => { self.advance(); TokenKind::LBrace }
            b'}' => { self.advance(); TokenKind::RBrace }
            b'[' => { self.advance(); TokenKind::LBracket }
            b']' => { self.advance(); TokenKind::RBracket }
            b';' => { self.advance(); TokenKind::Semicolon }
            b',' => { self.advance(); TokenKind::Comma }
            b':' => { self.advance(); TokenKind::Colon }
            b'@' => { self.advance(); TokenKind::At }
            b'#' => { self.advance(); TokenKind::Hash }
            b'~' => { self.advance(); TokenKind::Tilde }

            b'.' => self.scan_dot(),
            b'?' => self.scan_question(),
            b'+' => self.scan_plus(),
            b'-' => self.scan_minus(),
            b'*' => self.scan_star(),
            b'/' => self.scan_slash(),
            b'%' => self.scan_percent(),
            b'=' => self.scan_equals(),
            b'!' => self.scan_bang(),
            b'<' => self.scan_less_than(),
            b'>' => self.scan_greater_than(),
            b'&' => self.scan_ampersand(),
            b'|' => self.scan_pipe(),
            b'^' => self.scan_caret(),

            // Invalid character
            _ => {
                self.advance();
                TokenKind::Invalid
            }
        };

        // Update regex context based on the token we just scanned
        self.allow_regex = kind.can_start_expr() || matches!(
            kind,
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace
            | TokenKind::Comma | TokenKind::Semicolon | TokenKind::Colon
            | TokenKind::Question | TokenKind::Arrow
            | TokenKind::Eq | TokenKind::PlusEq | TokenKind::MinusEq
            | TokenKind::StarEq | TokenKind::SlashEq | TokenKind::PercentEq
            | TokenKind::AmpAmpEq | TokenKind::PipePipeEq | TokenKind::QuestionQuestionEq
        );

        self.make_token(kind)
    }

    /// Peek at the next token without consuming it.
    pub fn peek(&mut self) -> Token {
        let saved_pos = self.pos;
        let saved_start = self.token_start;
        let saved_regex = self.allow_regex;

        let token = self.next_token();

        self.pos = saved_pos;
        self.token_start = saved_start;
        self.allow_regex = saved_regex;

        token
    }

    // === Helper methods ===

    fn is_eof(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn current(&self) -> u8 {
        self.source.get(self.pos).copied().unwrap_or(0)
    }

    fn peek_char(&self) -> u8 {
        self.source.get(self.pos + 1).copied().unwrap_or(0)
    }

    fn peek_char_n(&self, n: usize) -> u8 {
        self.source.get(self.pos + n).copied().unwrap_or(0)
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn advance_n(&mut self, n: usize) {
        self.pos += n;
    }

    fn make_token(&self, kind: TokenKind) -> Token {
        Token::new(kind, Span::new(self.token_start as u32, self.pos as u32))
    }

    fn slice(&self, start: usize, end: usize) -> &'a str {
        // SAFETY: We only slice valid UTF-8 boundaries
        unsafe { std::str::from_utf8_unchecked(&self.source[start..end]) }
    }

    fn token_slice(&self) -> &'a str {
        self.slice(self.token_start, self.pos)
    }

    // === Whitespace and comments ===

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.current() {
                // Whitespace
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.advance();
                }
                // Comments
                b'/' if self.peek_char() == b'/' => {
                    self.skip_line_comment();
                }
                b'/' if self.peek_char() == b'*' => {
                    self.skip_block_comment();
                }
                _ => break,
            }
        }
    }

    fn skip_line_comment(&mut self) {
        self.advance_n(2); // Skip //
        while !self.is_eof() && self.current() != b'\n' {
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) {
        self.advance_n(2); // Skip /*
        while !self.is_eof() {
            if self.current() == b'*' && self.peek_char() == b'/' {
                self.advance_n(2);
                return;
            }
            self.advance();
        }
        // Unterminated block comment - will be reported as error during parsing
    }

    // === Token scanning ===

    fn scan_identifier(&mut self) -> TokenKind {
        while !self.is_eof() {
            match self.current() {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$' => {
                    self.advance();
                }
                // TODO: Handle Unicode identifiers
                _ => break,
            }
        }

        let ident = self.token_slice();

        // Check if it's a keyword
        keyword_from_str(ident).unwrap_or_else(|| TokenKind::Identifier(ident.to_string()))
    }

    fn scan_number(&mut self) -> TokenKind {
        let start = self.pos;

        // Handle different number formats
        if self.current() == b'0' {
            match self.peek_char() {
                b'x' | b'X' => return self.scan_hex_number(),
                b'b' | b'B' => return self.scan_binary_number(),
                b'o' | b'O' => return self.scan_octal_number(),
                _ => {}
            }
        }

        // Decimal integer part
        while self.current().is_ascii_digit() {
            self.advance();
        }

        // Decimal part
        if self.current() == b'.' && self.peek_char().is_ascii_digit() {
            self.advance(); // Skip .
            while self.current().is_ascii_digit() {
                self.advance();
            }
        }

        // Exponent part
        if self.current() == b'e' || self.current() == b'E' {
            self.advance();
            if self.current() == b'+' || self.current() == b'-' {
                self.advance();
            }
            while self.current().is_ascii_digit() {
                self.advance();
            }
        }

        // BigInt suffix
        if self.current() == b'n' {
            self.advance();
            return TokenKind::BigInt(self.slice(start, self.pos - 1).to_string());
        }

        let num_str = self.slice(start, self.pos);
        TokenKind::Number(num_str.parse().unwrap_or(f64::NAN))
    }

    fn scan_hex_number(&mut self) -> TokenKind {
        let start = self.pos;
        self.advance_n(2); // Skip 0x

        while self.current().is_ascii_hexdigit() {
            self.advance();
        }

        if self.current() == b'n' {
            self.advance();
            return TokenKind::BigInt(self.slice(start, self.pos - 1).to_string());
        }

        let hex_str = self.slice(start + 2, self.pos);
        let value = u64::from_str_radix(hex_str, 16).unwrap_or(0) as f64;
        TokenKind::Number(value)
    }

    fn scan_binary_number(&mut self) -> TokenKind {
        let start = self.pos;
        self.advance_n(2); // Skip 0b

        while self.current() == b'0' || self.current() == b'1' {
            self.advance();
        }

        if self.current() == b'n' {
            self.advance();
            return TokenKind::BigInt(self.slice(start, self.pos - 1).to_string());
        }

        let bin_str = self.slice(start + 2, self.pos);
        let value = u64::from_str_radix(bin_str, 2).unwrap_or(0) as f64;
        TokenKind::Number(value)
    }

    fn scan_octal_number(&mut self) -> TokenKind {
        let start = self.pos;
        self.advance_n(2); // Skip 0o

        while self.current() >= b'0' && self.current() <= b'7' {
            self.advance();
        }

        if self.current() == b'n' {
            self.advance();
            return TokenKind::BigInt(self.slice(start, self.pos - 1).to_string());
        }

        let oct_str = self.slice(start + 2, self.pos);
        let value = u64::from_str_radix(oct_str, 8).unwrap_or(0) as f64;
        TokenKind::Number(value)
    }

    fn scan_string(&mut self, quote: u8) -> TokenKind {
        self.advance(); // Skip opening quote
        let _start = self.pos;

        let mut value = String::new();
        while !self.is_eof() && self.current() != quote {
            if self.current() == b'\\' {
                self.advance();
                if !self.is_eof() {
                    value.push(self.scan_escape_sequence());
                }
            } else {
                value.push(self.current() as char);
                self.advance();
            }
        }

        if self.current() == quote {
            self.advance(); // Skip closing quote
        }

        TokenKind::String(value)
    }

    fn scan_escape_sequence(&mut self) -> char {
        let ch = self.current();
        self.advance();

        match ch {
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'\\' => '\\',
            b'\'' => '\'',
            b'"' => '"',
            b'0' => '\0',
            b'x' => self.scan_hex_escape(2),
            b'u' => {
                if self.current() == b'{' {
                    self.scan_unicode_escape_braces()
                } else {
                    self.scan_hex_escape(4)
                }
            }
            _ => ch as char,
        }
    }

    fn scan_hex_escape(&mut self, len: usize) -> char {
        let mut value = 0u32;
        for _ in 0..len {
            if let Some(digit) = (self.current() as char).to_digit(16) {
                value = value * 16 + digit;
                self.advance();
            } else {
                break;
            }
        }
        char::from_u32(value).unwrap_or('\u{FFFD}')
    }

    fn scan_unicode_escape_braces(&mut self) -> char {
        self.advance(); // Skip {
        let mut value = 0u32;
        while self.current() != b'}' && !self.is_eof() {
            if let Some(digit) = (self.current() as char).to_digit(16) {
                value = value * 16 + digit;
                self.advance();
            } else {
                break;
            }
        }
        if self.current() == b'}' {
            self.advance();
        }
        char::from_u32(value).unwrap_or('\u{FFFD}')
    }

    fn scan_template_head(&mut self) -> TokenKind {
        self.advance(); // Skip `
        let _start = self.pos;

        let mut value = String::new();
        while !self.is_eof() {
            match self.current() {
                b'`' => {
                    self.advance();
                    return TokenKind::TemplateNoSub(value);
                }
                b'$' if self.peek_char() == b'{' => {
                    self.advance_n(2);
                    return TokenKind::TemplateHead(value);
                }
                b'\\' => {
                    self.advance();
                    if !self.is_eof() {
                        value.push(self.scan_escape_sequence());
                    }
                }
                _ => {
                    value.push(self.current() as char);
                    self.advance();
                }
            }
        }

        // Unterminated template
        TokenKind::Invalid
    }

    /// Scan template middle or tail (called after `}` in template).
    pub fn scan_template_continuation(&mut self) -> TokenKind {
        self.token_start = self.pos;
        let mut value = String::new();

        while !self.is_eof() {
            match self.current() {
                b'`' => {
                    self.advance();
                    return self.make_token(TokenKind::TemplateTail(value)).kind;
                }
                b'$' if self.peek_char() == b'{' => {
                    self.advance_n(2);
                    return self.make_token(TokenKind::TemplateMiddle(value)).kind;
                }
                b'\\' => {
                    self.advance();
                    if !self.is_eof() {
                        value.push(self.scan_escape_sequence());
                    }
                }
                _ => {
                    value.push(self.current() as char);
                    self.advance();
                }
            }
        }

        TokenKind::Invalid
    }

    fn scan_regex(&mut self) -> TokenKind {
        self.advance(); // Skip opening /
        let pattern_start = self.pos;

        // Scan pattern
        let mut in_class = false;
        while !self.is_eof() {
            match self.current() {
                b'/' if !in_class => break,
                b'[' => {
                    in_class = true;
                    self.advance();
                }
                b']' => {
                    in_class = false;
                    self.advance();
                }
                b'\\' => {
                    self.advance();
                    if !self.is_eof() {
                        self.advance();
                    }
                }
                b'\n' | b'\r' => break, // Invalid - newline in regex
                _ => self.advance(),
            }
        }

        let pattern = self.slice(pattern_start, self.pos).to_string();

        if self.current() != b'/' {
            return TokenKind::Invalid;
        }
        self.advance(); // Skip closing /

        // Scan flags
        let flags_start = self.pos;
        while matches!(self.current(), b'g' | b'i' | b'm' | b's' | b'u' | b'y' | b'd' | b'v') {
            self.advance();
        }
        let flags = self.slice(flags_start, self.pos).to_string();

        TokenKind::Regex { pattern, flags }
    }

    // === Multi-character operators ===

    fn scan_dot(&mut self) -> TokenKind {
        self.advance();
        if self.current() == b'.' && self.peek_char() == b'.' {
            self.advance_n(2);
            TokenKind::Spread
        } else if self.current().is_ascii_digit() {
            // Number starting with .
            self.pos -= 1; // Back up to rescan
            self.scan_number()
        } else {
            TokenKind::Dot
        }
    }

    fn scan_question(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'?' => {
                self.advance();
                if self.current() == b'=' {
                    self.advance();
                    TokenKind::QuestionQuestionEq
                } else {
                    TokenKind::QuestionQuestion
                }
            }
            b'.' if !self.peek_char().is_ascii_digit() => {
                self.advance();
                TokenKind::QuestionDot
            }
            _ => TokenKind::Question,
        }
    }

    fn scan_plus(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'+' => { self.advance(); TokenKind::PlusPlus }
            b'=' => { self.advance(); TokenKind::PlusEq }
            _ => TokenKind::Plus,
        }
    }

    fn scan_minus(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'-' => { self.advance(); TokenKind::MinusMinus }
            b'=' => { self.advance(); TokenKind::MinusEq }
            _ => TokenKind::Minus,
        }
    }

    fn scan_star(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'*' => {
                self.advance();
                if self.current() == b'=' {
                    self.advance();
                    TokenKind::StarStarEq
                } else {
                    TokenKind::StarStar
                }
            }
            b'=' => { self.advance(); TokenKind::StarEq }
            _ => TokenKind::Star,
        }
    }

    fn scan_slash(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'=' => { self.advance(); TokenKind::SlashEq }
            _ if self.allow_regex => {
                self.pos -= 1; // Back up
                self.scan_regex()
            }
            _ => TokenKind::Slash,
        }
    }

    fn scan_percent(&mut self) -> TokenKind {
        self.advance();
        if self.current() == b'=' {
            self.advance();
            TokenKind::PercentEq
        } else {
            TokenKind::Percent
        }
    }

    fn scan_equals(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'=' => {
                self.advance();
                if self.current() == b'=' {
                    self.advance();
                    TokenKind::EqEqEq
                } else {
                    TokenKind::EqEq
                }
            }
            b'>' => { self.advance(); TokenKind::Arrow }
            _ => TokenKind::Eq,
        }
    }

    fn scan_bang(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'=' => {
                self.advance();
                if self.current() == b'=' {
                    self.advance();
                    TokenKind::BangEqEq
                } else {
                    TokenKind::BangEq
                }
            }
            _ => TokenKind::Bang,
        }
    }

    fn scan_less_than(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'<' => {
                self.advance();
                if self.current() == b'=' {
                    self.advance();
                    TokenKind::LtLtEq
                } else {
                    TokenKind::LtLt
                }
            }
            b'=' => { self.advance(); TokenKind::LtEq }
            _ => TokenKind::Lt,
        }
    }

    fn scan_greater_than(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'>' => {
                self.advance();
                match self.current() {
                    b'>' => {
                        self.advance();
                        if self.current() == b'=' {
                            self.advance();
                            TokenKind::GtGtGtEq
                        } else {
                            TokenKind::GtGtGt
                        }
                    }
                    b'=' => { self.advance(); TokenKind::GtGtEq }
                    _ => TokenKind::GtGt,
                }
            }
            b'=' => { self.advance(); TokenKind::GtEq }
            _ => TokenKind::Gt,
        }
    }

    fn scan_ampersand(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'&' => {
                self.advance();
                if self.current() == b'=' {
                    self.advance();
                    TokenKind::AmpAmpEq
                } else {
                    TokenKind::AmpAmp
                }
            }
            b'=' => { self.advance(); TokenKind::AmpEq }
            _ => TokenKind::Amp,
        }
    }

    fn scan_pipe(&mut self) -> TokenKind {
        self.advance();
        match self.current() {
            b'|' => {
                self.advance();
                if self.current() == b'=' {
                    self.advance();
                    TokenKind::PipePipeEq
                } else {
                    TokenKind::PipePipe
                }
            }
            b'=' => { self.advance(); TokenKind::PipeEq }
            _ => TokenKind::Pipe,
        }
    }

    fn scan_caret(&mut self) -> TokenKind {
        self.advance();
        if self.current() == b'=' {
            self.advance();
            TokenKind::CaretEq
        } else {
            TokenKind::Caret
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(source: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token();
            if matches!(token.kind, TokenKind::Eof) {
                break;
            }
            tokens.push(token.kind);
        }
        tokens
    }

    #[test]
    fn test_identifiers() {
        assert_eq!(
            tokenize("foo bar _baz $qux"),
            vec![
                TokenKind::Identifier("foo".into()),
                TokenKind::Identifier("bar".into()),
                TokenKind::Identifier("_baz".into()),
                TokenKind::Identifier("$qux".into()),
            ]
        );
    }

    #[test]
    fn test_keywords() {
        assert_eq!(
            tokenize("const let var function"),
            vec![TokenKind::Const, TokenKind::Let, TokenKind::Var, TokenKind::Function]
        );
    }

    #[test]
    fn test_numbers() {
        assert_eq!(
            tokenize("42 3.14 0xff 0b101 0o77"),
            vec![
                TokenKind::Number(42.0),
                TokenKind::Number(3.14),
                TokenKind::Number(255.0),
                TokenKind::Number(5.0),
                TokenKind::Number(63.0),
            ]
        );
    }

    #[test]
    fn test_strings() {
        assert_eq!(
            tokenize(r#""hello" 'world'"#),
            vec![
                TokenKind::String("hello".into()),
                TokenKind::String("world".into()),
            ]
        );
    }

    #[test]
    fn test_operators() {
        assert_eq!(
            tokenize("+ - * / % ** ++ --"),
            vec![
                TokenKind::Plus, TokenKind::Minus, TokenKind::Star, TokenKind::Slash,
                TokenKind::Percent, TokenKind::StarStar, TokenKind::PlusPlus, TokenKind::MinusMinus,
            ]
        );
    }

    #[test]
    fn test_comparison() {
        assert_eq!(
            tokenize("== === != !== < <= > >="),
            vec![
                TokenKind::EqEq, TokenKind::EqEqEq, TokenKind::BangEq, TokenKind::BangEqEq,
                TokenKind::Lt, TokenKind::LtEq, TokenKind::Gt, TokenKind::GtEq,
            ]
        );
    }

    #[test]
    fn test_arrow_function() {
        assert_eq!(
            tokenize("(x) => x"),
            vec![
                TokenKind::LParen,
                TokenKind::Identifier("x".into()),
                TokenKind::RParen,
                TokenKind::Arrow,
                TokenKind::Identifier("x".into()),
            ]
        );
    }

    #[test]
    fn test_comments() {
        assert_eq!(
            tokenize("a // line comment\nb /* block */ c"),
            vec![
                TokenKind::Identifier("a".into()),
                TokenKind::Identifier("b".into()),
                TokenKind::Identifier("c".into()),
            ]
        );
    }

    #[test]
    fn test_template_literal_no_sub() {
        assert_eq!(
            tokenize("`hello world`"),
            vec![TokenKind::TemplateNoSub("hello world".into())]
        );
    }
}
