use crate::parsing::token::Token;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct Location {
    pub beg_pos: usize,
    pub end_pos: usize,
}

impl Location {
    pub fn new(beg_pos: usize, end_pos: usize) -> Location {
        Location { beg_pos, end_pos }
    }

    pub fn is_valid(&self) -> bool {
        self.beg_pos <= self.end_pos
    }
}

#[derive(Clone, Debug)]
pub struct TokenDesc {
    pub token: Token,
    pub location: Location,
    pub literal: String,
    pub smi_value: i32,
    pub number_value: f64,
}

impl Default for TokenDesc {
    fn default() -> TokenDesc {
        TokenDesc {
            token: Token::Uninitialized,
            location: Location::default(),
            literal: String::new(),
            smi_value: 0,
            number_value: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannerError {
    pub message: String,
    pub location: Location,
}

pub struct Scanner {
    source: Vec<char>,
    cursor: usize,
    c0: Option<char>,

    current: TokenDesc,
    next: TokenDesc,
    next_next: TokenDesc,

    has_line_terminator_before_next: bool,
    has_line_terminator_after_next: bool,

    error: Option<ScannerError>,
}

impl Scanner {
    pub fn new(source: &str) -> Scanner {
        let mut scanner = Scanner {
            source: source.chars().collect(),
            cursor: 0,
            c0: None,
            current: TokenDesc::default(),
            next: TokenDesc::default(),
            next_next: TokenDesc::default(),
            has_line_terminator_before_next: false,
            has_line_terminator_after_next: false,
            error: None,
        };
        scanner.advance();
        scanner.scan_into_next();
        scanner
    }

    /// `Scanner::current_token`
    #[inline]
    pub fn current_token(&self) -> Token {
        self.current.token
    }

    #[inline]
    pub fn peek(&self) -> Token {
        self.next.token
    }

    pub fn peek_ahead(&mut self) -> Token {
        if self.next_next.token != Token::Uninitialized {
            return self.next_next.token;
        }
        let saved_line_terminator = self.has_line_terminator_before_next;
        self.has_line_terminator_before_next = false;
        let desc = self.scan_token();
        self.has_line_terminator_after_next = self.has_line_terminator_before_next;
        self.has_line_terminator_before_next = saved_line_terminator;
        self.next_next = desc;
        self.next_next.token
    }

    pub fn next(&mut self) -> Token {
        self.current = std::mem::take(&mut self.next);
        if self.next_next.token != Token::Uninitialized {
            self.next = std::mem::take(&mut self.next_next);
            self.next_next = TokenDesc::default();
            self.has_line_terminator_before_next = self.has_line_terminator_after_next;
            self.has_line_terminator_after_next = false;
        } else {
            self.has_line_terminator_before_next = false;
            self.scan_into_next();
        }
        self.current.token
    }

    fn scan_into_next(&mut self) {
        self.next = self.scan_token();
    }

    #[inline]
    pub fn location(&self) -> Location {
        self.current.location
    }

    /// `Scanner::peek_location`
    #[inline]
    pub fn peek_location(&self) -> Location {
        self.next.location
    }

    #[inline]
    pub fn current_literal(&self) -> &str {
        &self.current.literal
    }

    #[inline]
    pub fn next_literal(&self) -> &str {
        &self.next.literal
    }

    #[inline]
    pub fn smi_value(&self) -> i32 {
        debug_assert_eq!(self.current.token, Token::Smi);
        self.current.smi_value
    }

    #[inline]
    pub fn number_value(&self) -> f64 {
        match self.current.token {
            Token::Smi => self.current.smi_value as f64,
            _ => self.current.number_value,
        }
    }

    #[inline]
    pub fn has_line_terminator_before_next(&self) -> bool {
        self.has_line_terminator_before_next
    }

    pub fn error(&self) -> Option<&ScannerError> {
        self.error.as_ref()
    }

    fn report_error(&mut self, message: impl Into<String>, location: Location) {
        if self.error.is_none() {
            self.error = Some(ScannerError {
                message: message.into(),
                location,
            });
        }
    }

    // -- the character stream -----------------------------------------------

    /// `Scanner::Advance`
    #[inline]
    fn advance(&mut self) {
        self.c0 = self.source.get(self.cursor).copied();
        self.cursor += 1;
    }

    #[inline]
    fn position(&self) -> usize {
        self.cursor.saturating_sub(1)
    }

    #[inline]
    fn peek_char(&self) -> Option<char> {
        self.source.get(self.cursor).copied()
    }

    #[inline]
    fn advance_if(&mut self, expected: char) -> bool {
        if self.c0 == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    #[inline]
    fn is_line_terminator(c: char) -> bool {
        matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
    }

    #[inline]
    fn is_whitespace(c: char) -> bool {
        matches!(
            c,
            '\t' | '\u{000b}' | '\u{000c}' | ' ' | '\u{00a0}' | '\u{feff}' | '\u{1680}' | '\u{2000}'
                ..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
        ) || Self::is_line_terminator(c)
    }

    /// `Scanner::IsIdentifierStart`
    #[inline]
    fn is_identifier_start(c: char) -> bool {
        c == '$' || c == '_' || c.is_alphabetic()
    }

    /// `Scanner::IsIdentifierPart`
    #[inline]
    fn is_identifier_part(c: char) -> bool {
        c == '$' || c == '_' || c.is_alphanumeric() || c == '\u{200c}' || c == '\u{200d}'
    }

    // -- skipping -----------------------------------------------------------

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while let Some(c) = self.c0 {
                if !Self::is_whitespace(c) {
                    break;
                }
                if Self::is_line_terminator(c) {
                    self.has_line_terminator_before_next = true;
                }
                self.advance();
            }
            if self.c0 == Some('/') {
                match self.peek_char() {
                    Some('/') => {
                        self.skip_single_line_comment();
                        continue;
                    }
                    Some('*') => {
                        self.skip_multi_line_comment();
                        continue;
                    }
                    _ => {}
                }
            }
            return;
        }
    }

    fn skip_single_line_comment(&mut self) {
        self.advance(); // '/'
        self.advance(); // '/'
        while let Some(c) = self.c0 {
            if Self::is_line_terminator(c) {
                break;
            }
            self.advance();
        }
    }

    fn skip_multi_line_comment(&mut self) {
        let start = self.position();
        self.advance(); // '/'
        self.advance(); // '*'
        while let Some(c) = self.c0 {
            if Self::is_line_terminator(c) {
                self.has_line_terminator_before_next = true;
            }
            if c == '*' && self.peek_char() == Some('/') {
                self.advance();
                self.advance();
                return;
            }
            self.advance();
        }
        self.report_error(
            "unterminated comment",
            Location::new(start, self.position()),
        );
    }

    // -- scanning -----------------------------------------------------------

    fn scan_token(&mut self) -> TokenDesc {
        self.skip_whitespace_and_comments();
        let beg_pos = self.position();

        let Some(c) = self.c0 else {
            return self.make(Token::Eos, beg_pos);
        };

        match c {
            '(' => {
                self.advance();
                self.make(Token::LeftParen, beg_pos)
            }
            ')' => {
                self.advance();
                self.make(Token::RightParen, beg_pos)
            }
            '[' => {
                self.advance();
                self.make(Token::LeftBracket, beg_pos)
            }
            ']' => {
                self.advance();
                self.make(Token::RightBracket, beg_pos)
            }
            '{' => {
                self.advance();
                self.make(Token::LeftBrace, beg_pos)
            }
            '}' => {
                self.advance();
                self.make(Token::RightBrace, beg_pos)
            }
            ';' => {
                self.advance();
                self.make(Token::Semicolon, beg_pos)
            }
            ',' => {
                self.advance();
                self.make(Token::Comma, beg_pos)
            }
            ':' => {
                self.advance();
                self.make(Token::Colon, beg_pos)
            }
            '~' => {
                self.advance();
                self.make(Token::BitNot, beg_pos)
            }

            '.' => {
                self.advance();
                if self.c0 == Some('.') && self.peek_char() == Some('.') {
                    self.advance();
                    self.advance();
                    return self.make(Token::Ellipsis, beg_pos);
                }
                if self.c0.is_some_and(|c| c.is_ascii_digit()) {
                    return self.scan_number(beg_pos, true);
                }
                self.make(Token::Period, beg_pos)
            }

            '?' => {
                self.advance();
                if self.advance_if('?') {
                    if self.advance_if('=') {
                        return self.make(Token::AssignNullish, beg_pos);
                    }
                    return self.make(Token::Nullish, beg_pos);
                }
                if self.c0 == Some('.') && !self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                    self.advance();
                    return self.make(Token::QuestionPeriod, beg_pos);
                }
                self.make(Token::Conditional, beg_pos)
            }

            '+' => {
                self.advance();
                if self.advance_if('+') {
                    return self.make(Token::Inc, beg_pos);
                }
                if self.advance_if('=') {
                    return self.make(Token::AssignAdd, beg_pos);
                }
                self.make(Token::Add, beg_pos)
            }
            '-' => {
                self.advance();
                if self.advance_if('-') {
                    return self.make(Token::Dec, beg_pos);
                }
                if self.advance_if('=') {
                    return self.make(Token::AssignSub, beg_pos);
                }
                self.make(Token::Sub, beg_pos)
            }
            '*' => {
                self.advance();
                if self.advance_if('*') {
                    if self.advance_if('=') {
                        return self.make(Token::AssignExp, beg_pos);
                    }
                    return self.make(Token::Exp, beg_pos);
                }
                if self.advance_if('=') {
                    return self.make(Token::AssignMul, beg_pos);
                }
                self.make(Token::Mul, beg_pos)
            }
            '/' => {
                self.advance();
                if self.advance_if('=') {
                    return self.make(Token::AssignDiv, beg_pos);
                }
                self.make(Token::Div, beg_pos)
            }
            '%' => {
                self.advance();
                if self.advance_if('=') {
                    return self.make(Token::AssignMod, beg_pos);
                }
                self.make(Token::Mod, beg_pos)
            }

            '&' => {
                self.advance();
                if self.advance_if('&') {
                    if self.advance_if('=') {
                        return self.make(Token::AssignAnd, beg_pos);
                    }
                    return self.make(Token::And, beg_pos);
                }
                if self.advance_if('=') {
                    return self.make(Token::AssignBitAnd, beg_pos);
                }
                self.make(Token::BitAnd, beg_pos)
            }
            '|' => {
                self.advance();
                if self.advance_if('|') {
                    if self.advance_if('=') {
                        return self.make(Token::AssignOr, beg_pos);
                    }
                    return self.make(Token::Or, beg_pos);
                }
                if self.advance_if('=') {
                    return self.make(Token::AssignBitOr, beg_pos);
                }
                self.make(Token::BitOr, beg_pos)
            }
            '^' => {
                self.advance();
                if self.advance_if('=') {
                    return self.make(Token::AssignBitXor, beg_pos);
                }
                self.make(Token::BitXor, beg_pos)
            }

            '=' => {
                self.advance();
                if self.advance_if('=') {
                    if self.advance_if('=') {
                        return self.make(Token::EqStrict, beg_pos);
                    }
                    return self.make(Token::Eq, beg_pos);
                }
                if self.advance_if('>') {
                    return self.make(Token::Arrow, beg_pos);
                }
                self.make(Token::Assign, beg_pos)
            }
            '!' => {
                self.advance();
                if self.advance_if('=') {
                    if self.advance_if('=') {
                        return self.make(Token::NotEqStrict, beg_pos);
                    }
                    return self.make(Token::NotEq, beg_pos);
                }
                self.make(Token::Not, beg_pos)
            }
            '<' => {
                self.advance();
                if self.advance_if('=') {
                    return self.make(Token::LessThanEq, beg_pos);
                }
                if self.advance_if('<') {
                    if self.advance_if('=') {
                        return self.make(Token::AssignShl, beg_pos);
                    }
                    return self.make(Token::Shl, beg_pos);
                }
                self.make(Token::LessThan, beg_pos)
            }
            '>' => {
                self.advance();
                if self.advance_if('=') {
                    return self.make(Token::GreaterThanEq, beg_pos);
                }
                if self.advance_if('>') {
                    if self.advance_if('>') {
                        if self.advance_if('=') {
                            return self.make(Token::AssignShr, beg_pos);
                        }
                        return self.make(Token::Shr, beg_pos);
                    }
                    if self.advance_if('=') {
                        return self.make(Token::AssignSar, beg_pos);
                    }
                    return self.make(Token::Sar, beg_pos);
                }
                self.make(Token::GreaterThan, beg_pos)
            }

            '"' | '\'' => self.scan_string(beg_pos),

            '0'..='9' => self.scan_number(beg_pos, false),

            '`' => {
                self.advance();
                self.report_error(
                    "template literals are not implemented",
                    Location::new(beg_pos, self.position()),
                );
                self.make(Token::Illegal, beg_pos)
            }

            c if Self::is_identifier_start(c) => self.scan_identifier_or_keyword(beg_pos),

            other => {
                self.advance();
                self.report_error(
                    format!("unexpected character {other:?}"),
                    Location::new(beg_pos, self.position()),
                );
                self.make(Token::Illegal, beg_pos)
            }
        }
    }

    fn make(&self, token: Token, beg_pos: usize) -> TokenDesc {
        TokenDesc {
            token,
            location: Location::new(beg_pos, self.position()),
            literal: String::new(),
            smi_value: 0,
            number_value: 0.0,
        }
    }

    /// `Scanner::ScanIdentifierOrKeyword`
    fn scan_identifier_or_keyword(&mut self, beg_pos: usize) -> TokenDesc {
        let mut literal = String::new();
        while let Some(c) = self.c0 {
            if !Self::is_identifier_part(c) {
                break;
            }
            literal.push(c);
            self.advance();
        }
        let token = keyword_or_identifier_token(&literal);
        TokenDesc {
            token,
            location: Location::new(beg_pos, self.position()),
            literal,
            smi_value: 0,
            number_value: 0.0,
        }
    }

    /// `Scanner::ScanString`
    fn scan_string(&mut self, beg_pos: usize) -> TokenDesc {
        let quote = self.c0.unwrap();
        self.advance();
        let mut literal = String::new();
        loop {
            let Some(c) = self.c0 else {
                self.report_error(
                    "unterminated string literal",
                    Location::new(beg_pos, self.position()),
                );
                return self.make(Token::Illegal, beg_pos);
            };
            if c == quote {
                self.advance();
                break;
            }
            if Self::is_line_terminator(c) {
                self.report_error(
                    "unterminated string literal",
                    Location::new(beg_pos, self.position()),
                );
                return self.make(Token::Illegal, beg_pos);
            }
            if c == '\\' {
                self.advance();
                self.scan_escape(&mut literal, beg_pos);
                continue;
            }
            literal.push(c);
            self.advance();
        }
        TokenDesc {
            token: Token::String,
            location: Location::new(beg_pos, self.position()),
            literal,
            smi_value: 0,
            number_value: 0.0,
        }
    }

    /// `Scanner::ScanEscape`
    fn scan_escape(&mut self, out: &mut String, beg_pos: usize) {
        let Some(c) = self.c0 else {
            self.report_error(
                "unterminated string literal",
                Location::new(beg_pos, self.position()),
            );
            return;
        };
        self.advance();
        match c {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'b' => out.push('\u{0008}'),
            'f' => out.push('\u{000c}'),
            'v' => out.push('\u{000b}'),
            '0' if !self.c0.is_some_and(|c| c.is_ascii_digit()) => out.push('\0'),
            'x' => {
                let mut value = 0u32;
                for _ in 0..2 {
                    match self.c0.and_then(|c| c.to_digit(16)) {
                        Some(digit) => {
                            value = value * 16 + digit;
                            self.advance();
                        }
                        None => {
                            self.report_error(
                                "invalid hexadecimal escape sequence",
                                Location::new(beg_pos, self.position()),
                            );
                            return;
                        }
                    }
                }
                out.push(char::from_u32(value).unwrap_or('\u{fffd}'));
            }
            'u' => self.scan_unicode_escape(out, beg_pos),
            c if Self::is_line_terminator(c) => {
                if c == '\r' {
                    self.advance_if('\n');
                }
            }
            other => out.push(other),
        }
    }

    fn scan_unicode_escape(&mut self, out: &mut String, beg_pos: usize) {
        if self.advance_if('{') {
            let mut value = 0u32;
            let mut digits = 0;
            while let Some(digit) = self.c0.and_then(|c| c.to_digit(16)) {
                value = value * 16 + digit;
                digits += 1;
                self.advance();
            }
            if digits == 0 || !self.advance_if('}') || char::from_u32(value).is_none() {
                self.report_error(
                    "invalid Unicode escape sequence",
                    Location::new(beg_pos, self.position()),
                );
                return;
            }
            out.push(char::from_u32(value).unwrap());
            return;
        }
        let mut value = 0u32;
        for _ in 0..4 {
            match self.c0.and_then(|c| c.to_digit(16)) {
                Some(digit) => {
                    value = value * 16 + digit;
                    self.advance();
                }
                None => {
                    self.report_error(
                        "invalid Unicode escape sequence",
                        Location::new(beg_pos, self.position()),
                    );
                    return;
                }
            }
        }
        match char::from_u32(value) {
            Some(c) => out.push(c),
            None => out.push('\u{fffd}'),
        }
    }

    fn scan_number(&mut self, beg_pos: usize, seen_period: bool) -> TokenDesc {
        let mut text = String::new();
        let mut is_integer = !seen_period;

        if seen_period {
            text.push('0');
            text.push('.');
            while self.c0.is_some_and(|c| c.is_ascii_digit()) {
                text.push(self.c0.unwrap());
                self.advance();
            }
        } else if self.c0 == Some('0') {
            text.push('0');
            self.advance();
            let radix = match self.c0 {
                Some('x') | Some('X') => Some(16),
                Some('o') | Some('O') => Some(8),
                Some('b') | Some('B') => Some(2),
                _ => None,
            };
            if let Some(radix) = radix {
                self.advance();
                let mut digits = String::new();
                while let Some(c) = self.c0 {
                    if c.to_digit(radix).is_none() {
                        break;
                    }
                    digits.push(c);
                    self.advance();
                }
                if digits.is_empty() {
                    self.report_error(
                        "missing digits after radix prefix",
                        Location::new(beg_pos, self.position()),
                    );
                    return self.make(Token::Illegal, beg_pos);
                }
                let value = match u128::from_str_radix(&digits, radix) {
                    Ok(value) => value as f64,
                    Err(_) => f64::INFINITY,
                };
                return self.finish_number(beg_pos, value, true);
            }
            while self.c0.is_some_and(|c| c.is_ascii_digit()) {
                text.push(self.c0.unwrap());
                self.advance();
            }
        } else {
            while self.c0.is_some_and(|c| c.is_ascii_digit()) {
                text.push(self.c0.unwrap());
                self.advance();
            }
        }

        if !seen_period && self.c0 == Some('.') {
            is_integer = false;
            text.push('.');
            self.advance();
            while self.c0.is_some_and(|c| c.is_ascii_digit()) {
                text.push(self.c0.unwrap());
                self.advance();
            }
        }

        if matches!(self.c0, Some('e') | Some('E')) {
            is_integer = false;
            text.push('e');
            self.advance();
            if matches!(self.c0, Some('+') | Some('-')) {
                text.push(self.c0.unwrap());
                self.advance();
            }
            if !self.c0.is_some_and(|c| c.is_ascii_digit()) {
                self.report_error(
                    "missing exponent digits",
                    Location::new(beg_pos, self.position()),
                );
                return self.make(Token::Illegal, beg_pos);
            }
            while self.c0.is_some_and(|c| c.is_ascii_digit()) {
                text.push(self.c0.unwrap());
                self.advance();
            }
        }

        if self.c0.is_some_and(Self::is_identifier_start) {
            self.report_error(
                "identifier starts immediately after a numeric literal",
                Location::new(beg_pos, self.position()),
            );
            return self.make(Token::Illegal, beg_pos);
        }

        let value: f64 = text.parse().unwrap_or(f64::NAN);
        self.finish_number(beg_pos, value, is_integer)
    }

    fn finish_number(&mut self, beg_pos: usize, value: f64, is_integer: bool) -> TokenDesc {
        let location = Location::new(beg_pos, self.position());
        if is_integer {
            if let Some(smi) = crate::objects::smi::Smi::try_from_f64(value) {
                return TokenDesc {
                    token: Token::Smi,
                    location,
                    literal: String::new(),
                    smi_value: crate::objects::smi::Smi::to_int(smi),
                    number_value: value,
                };
            }
        }
        TokenDesc {
            token: Token::Number,
            location,
            literal: String::new(),
            smi_value: 0,
            number_value: value,
        }
    }

    pub fn tokenize_all(source: &str) -> (Vec<TokenDesc>, Option<ScannerError>) {
        let mut scanner = Scanner::new(source);
        let mut tokens = Vec::new();
        loop {
            let token = scanner.next();
            tokens.push(scanner.current.clone());
            if token == Token::Eos || token == Token::Illegal {
                break;
            }
        }
        let error = scanner.error.clone();
        (tokens, error)
    }
}

fn keyword_or_identifier_token(literal: &str) -> Token {
    match literal {
        "break" => Token::Break,
        "case" => Token::Case,
        "catch" => Token::Catch,
        "class" => Token::Class,
        "const" => Token::Const,
        "continue" => Token::Continue,
        "debugger" => Token::Debugger,
        "default" => Token::Default,
        "delete" => Token::Delete,
        "do" => Token::Do,
        "else" => Token::Else,
        "enum" => Token::Enum,
        "export" => Token::Export,
        "extends" => Token::Extends,
        "false" => Token::FalseLiteral,
        "finally" => Token::Finally,
        "for" => Token::For,
        "function" => Token::Function,
        "if" => Token::If,
        "import" => Token::Import,
        "in" => Token::In,
        "instanceof" => Token::InstanceOf,
        "new" => Token::New,
        "null" => Token::NullLiteral,
        "return" => Token::Return,
        "super" => Token::Super,
        "switch" => Token::Switch,
        "this" => Token::This,
        "throw" => Token::Throw,
        "true" => Token::TrueLiteral,
        "try" => Token::Try,
        "typeof" => Token::TypeOf,
        "var" => Token::Var,
        "void" => Token::Void,
        "while" => Token::While,
        "with" => Token::With,
        "accessor" => Token::Accessor,
        "async" => Token::Async,
        "await" => Token::Await,
        "get" => Token::Get,
        "let" => Token::Let,
        "of" => Token::Of,
        "set" => Token::Set,
        "static" => Token::Static,
        "using" => Token::Using,
        "yield" => Token::Yield,
        "implements" | "interface" | "package" | "private" | "protected" | "public" => {
            Token::FutureStrictReservedWord
        }
        _ => Token::Identifier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(source: &str) -> Vec<Token> {
        let (descs, error) = Scanner::tokenize_all(source);
        assert!(error.is_none(), "unexpected scanner error: {error:?}");
        descs.iter().map(|d| d.token).collect()
    }

    fn scan_one(source: &str) -> TokenDesc {
        let (descs, error) = Scanner::tokenize_all(source);
        assert!(error.is_none(), "unexpected scanner error: {error:?}");
        assert_eq!(descs.len(), 2, "expected exactly one token in {source:?}");
        descs[0].clone()
    }

    #[test]
    fn empty_input_is_just_end_of_stream() {
        assert_eq!(tokens(""), vec![Token::Eos]);
        assert_eq!(tokens("   \n\t  "), vec![Token::Eos]);
    }

    #[test]
    fn punctuators_scan_longest_first() {
        assert_eq!(scan_one(">").token, Token::GreaterThan);
        assert_eq!(scan_one(">=").token, Token::GreaterThanEq);
        assert_eq!(scan_one(">>").token, Token::Sar);
        assert_eq!(scan_one(">>=").token, Token::AssignSar);
        assert_eq!(scan_one(">>>").token, Token::Shr);
        assert_eq!(scan_one(">>>=").token, Token::AssignShr);

        assert_eq!(scan_one("=").token, Token::Assign);
        assert_eq!(scan_one("==").token, Token::Eq);
        assert_eq!(scan_one("===").token, Token::EqStrict);
        assert_eq!(scan_one("=>").token, Token::Arrow);

        assert_eq!(scan_one("!").token, Token::Not);
        assert_eq!(scan_one("!=").token, Token::NotEq);
        assert_eq!(scan_one("!==").token, Token::NotEqStrict);

        assert_eq!(scan_one("*").token, Token::Mul);
        assert_eq!(scan_one("**").token, Token::Exp);
        assert_eq!(scan_one("**=").token, Token::AssignExp);

        assert_eq!(scan_one("&").token, Token::BitAnd);
        assert_eq!(scan_one("&&").token, Token::And);
        assert_eq!(scan_one("&&=").token, Token::AssignAnd);

        assert_eq!(scan_one("?").token, Token::Conditional);
        assert_eq!(scan_one("??").token, Token::Nullish);
        assert_eq!(scan_one("??=").token, Token::AssignNullish);
        assert_eq!(scan_one("?.").token, Token::QuestionPeriod);

        assert_eq!(scan_one(".").token, Token::Period);
        assert_eq!(scan_one("...").token, Token::Ellipsis);
    }

    #[test]
    fn optional_chaining_versus_a_conditional_followed_by_a_number() {
        assert_eq!(
            tokens("a?.5:1"),
            vec![
                Token::Identifier,
                Token::Conditional,
                Token::Number,
                Token::Colon,
                Token::Smi,
                Token::Eos
            ]
        );
        assert_eq!(
            tokens("a?.b"),
            vec![
                Token::Identifier,
                Token::QuestionPeriod,
                Token::Identifier,
                Token::Eos
            ]
        );
    }

    #[test]
    fn keywords_are_distinguished_from_identifiers() {
        assert_eq!(scan_one("function").token, Token::Function);
        assert_eq!(scan_one("if").token, Token::If);
        assert_eq!(scan_one("typeof").token, Token::TypeOf);
        assert_eq!(scan_one("null").token, Token::NullLiteral);
        assert_eq!(scan_one("true").token, Token::TrueLiteral);
        assert_eq!(scan_one("instanceof").token, Token::InstanceOf);
        assert_eq!(scan_one("iff").token, Token::Identifier);
        assert_eq!(scan_one("functio").token, Token::Identifier);
        assert_eq!(scan_one("Function").token, Token::Identifier);
        assert_eq!(scan_one("nullish").token, Token::Identifier);
    }

    #[test]
    fn contextual_keywords_get_their_own_token() {
        for (source, expected) in [
            ("let", Token::Let),
            ("of", Token::Of),
            ("async", Token::Async),
            ("get", Token::Get),
            ("yield", Token::Yield),
            ("static", Token::Static),
        ] {
            assert_eq!(scan_one(source).token, expected, "{source}");
            assert!(
                expected.is_valid_identifier(false, false, false),
                "{source}"
            );
        }
        assert_eq!(
            scan_one("implements").token,
            Token::FutureStrictReservedWord
        );
    }

    #[test]
    fn identifiers_may_start_with_a_dollar_or_underscore() {
        for source in ["$", "_", "$foo", "_bar", "a1", "ümlaut", "日本語"] {
            let desc = scan_one(source);
            assert_eq!(desc.token, Token::Identifier, "{source}");
            assert_eq!(desc.literal, source);
        }
        assert_ne!(scan_one("1").token, Token::Identifier);
    }

    #[test]
    fn integers_that_fit_become_smis_and_the_rest_become_doubles() {
        assert_eq!(scan_one("0").token, Token::Smi);
        assert_eq!(scan_one("0").smi_value, 0);
        assert_eq!(scan_one("42").smi_value, 42);
        assert_eq!(scan_one("2147483647").smi_value, i32::MAX);

        let big = scan_one("2147483648");
        assert_eq!(big.token, Token::Number);
        assert_eq!(big.number_value, 2147483648.0);

        // Not integers.
        assert_eq!(scan_one("1.5").token, Token::Number);
        assert_eq!(scan_one("1.5").number_value, 1.5);
        assert_eq!(scan_one("1e3").token, Token::Number);
        assert_eq!(scan_one("1e3").number_value, 1000.0);
    }

    #[test]
    fn numeric_forms() {
        for (source, expected) in [
            (".5", 0.5),
            ("0.5", 0.5),
            ("1.", 1.0),
            ("1e3", 1000.0),
            ("1E3", 1000.0),
            ("1e+3", 1000.0),
            ("1e-3", 0.001),
            ("1.5e2", 150.0),
            ("0x1f", 31.0),
            ("0X1F", 31.0),
            ("0o17", 15.0),
            ("0b1011", 11.0),
        ] {
            let desc = scan_one(source);
            let value = match desc.token {
                Token::Smi => desc.smi_value as f64,
                Token::Number => desc.number_value,
                other => panic!("{source} scanned as {}", other.name()),
            };
            assert_eq!(value, expected, "{source}");
        }
    }

    #[test]
    fn a_period_after_digits_is_part_of_the_number_but_not_after_an_identifier() {
        assert_eq!(tokens("1.5"), vec![Token::Number, Token::Eos]);
        assert_eq!(
            tokens("a.b"),
            vec![
                Token::Identifier,
                Token::Period,
                Token::Identifier,
                Token::Eos
            ]
        );
        assert_eq!(
            tokens("1..x"),
            vec![Token::Number, Token::Period, Token::Identifier, Token::Eos]
        );
    }

    #[test]
    fn string_literals_and_their_escapes() {
        assert_eq!(scan_one("'hello'").literal, "hello");
        assert_eq!(scan_one("\"hello\"").literal, "hello");
        assert_eq!(scan_one("''").literal, "");
        assert_eq!(scan_one(r"'a\nb'").literal, "a\nb");
        assert_eq!(scan_one(r"'a\tb'").literal, "a\tb");
        assert_eq!(scan_one(r"'a\\b'").literal, "a\\b");
        assert_eq!(scan_one(r"'it\'s'").literal, "it's");
        assert_eq!(scan_one(r#""say \"hi\"""#).literal, "say \"hi\"");
        assert_eq!(scan_one(r"'\x41'").literal, "A");
        assert_eq!(scan_one(r"'A'").literal, "A");
        assert_eq!(scan_one(r"'\u{1f600}'").literal, "😀");
        assert_eq!(scan_one(r"'\0'").literal, "\0");
        assert_eq!(scan_one(r"'\q'").literal, "q");
        assert_eq!(scan_one("'say \"hi\"'").literal, "say \"hi\"");
    }

    #[test]
    fn a_backslash_before_a_newline_is_a_line_continuation() {
        assert_eq!(scan_one("'a\\\nb'").literal, "ab");
    }

    #[test]
    fn an_unterminated_string_is_an_error_not_a_panic() {
        let (_, error) = Scanner::tokenize_all("'oops");
        assert!(error.is_some());
        assert!(error.unwrap().message.contains("unterminated string"));

        let (_, error) = Scanner::tokenize_all("'oops\n'");
        assert!(error.is_some());
    }

    #[test]
    fn comments_are_invisible_to_the_token_stream() {
        assert_eq!(
            tokens("a // comment\nb"),
            vec![Token::Identifier, Token::Identifier, Token::Eos]
        );
        assert_eq!(
            tokens("a /* comment */ b"),
            vec![Token::Identifier, Token::Identifier, Token::Eos]
        );
        assert_eq!(tokens("// only a comment"), vec![Token::Eos]);
        assert_eq!(
            tokens("a /* multi\nline */ b"),
            vec![Token::Identifier, Token::Identifier, Token::Eos]
        );
        assert_eq!(
            tokens("a / b"),
            vec![Token::Identifier, Token::Div, Token::Identifier, Token::Eos]
        );
    }

    #[test]
    fn an_unterminated_block_comment_is_an_error() {
        let (_, error) = Scanner::tokenize_all("a /* oops");
        assert!(error.is_some());
        assert!(error.unwrap().message.contains("unterminated comment"));
    }

    #[test]
    fn line_terminators_are_recorded_for_semicolon_insertion() {
        let mut scanner = Scanner::new("a\nb c");
        assert_eq!(scanner.next(), Token::Identifier); // a
        assert!(
            scanner.has_line_terminator_before_next(),
            "a newline separates `a` from `b`"
        );
        assert_eq!(scanner.next(), Token::Identifier); // b
        assert!(
            !scanner.has_line_terminator_before_next(),
            "only a space separates `b` from `c`"
        );
        assert_eq!(scanner.next(), Token::Identifier); // c
    }

    #[test]
    fn a_newline_inside_a_block_comment_still_counts() {
        let mut scanner = Scanner::new("return /*\n*/ 1");
        assert_eq!(scanner.next(), Token::Return);
        assert!(scanner.has_line_terminator_before_next());
    }

    #[test]
    fn all_four_javascript_line_terminators_count() {
        for separator in ["\n", "\r", "\u{2028}", "\u{2029}"] {
            let source = format!("a{separator}b");
            let mut scanner = Scanner::new(&source);
            scanner.next();
            assert!(
                scanner.has_line_terminator_before_next(),
                "{separator:?} should be a line terminator"
            );
        }
        let mut scanner = Scanner::new("a\u{00a0}b");
        scanner.next();
        assert!(
            !scanner.has_line_terminator_before_next(),
            "a no-break space is whitespace but not a terminator"
        );
    }

    #[test]
    fn lookahead_is_two_tokens_deep_and_does_not_disturb_the_stream() {
        let mut scanner = Scanner::new("a + b");
        assert_eq!(scanner.peek(), Token::Identifier);
        assert_eq!(scanner.peek_ahead(), Token::Add);
        assert_eq!(scanner.peek(), Token::Identifier);
        assert_eq!(scanner.peek_ahead(), Token::Add);
        assert_eq!(scanner.next(), Token::Identifier);
        assert_eq!(scanner.next(), Token::Add);
        assert_eq!(scanner.next(), Token::Identifier);
        assert_eq!(scanner.next(), Token::Eos);
    }

    #[test]
    fn lookahead_preserves_the_line_terminator_flag() {
        let mut scanner = Scanner::new("a\nb c");
        scanner.next(); // a
        assert!(scanner.has_line_terminator_before_next());
        assert_eq!(scanner.peek_ahead(), Token::Identifier); // c
        assert!(
            scanner.has_line_terminator_before_next(),
            "the flag for the *next* token must survive a peek_ahead"
        );
        scanner.next(); // b
        assert!(!scanner.has_line_terminator_before_next());
    }

    #[test]
    fn locations_point_at_the_token() {
        let (descs, _) = Scanner::tokenize_all("let x = 42;");
        let spellings: Vec<&str> = descs
            .iter()
            .map(|d| {
                let source: Vec<char> = "let x = 42;".chars().collect();
                let text: String = source[d.location.beg_pos..d.location.end_pos.min(source.len())]
                    .iter()
                    .collect();
                Box::leak(text.into_boxed_str()) as &str
            })
            .collect();
        assert_eq!(spellings[0], "let");
        assert_eq!(spellings[1], "x");
        assert_eq!(spellings[2], "=");
        assert_eq!(spellings[3], "42");
        assert_eq!(spellings[4], ";");
    }

    #[test]
    fn a_realistic_program_scans_cleanly() {
        let source = r#"
            function factorial(n) {
              if (n <= 1) return 1;
              return n * factorial(n - 1);
            }
            var total = 0;
            for (var i = 0; i < 10; i = i + 1) {
              total += factorial(i);
            }
            var message = "total: ";
        "#;
        let (descs, error) = Scanner::tokenize_all(source);
        assert!(error.is_none(), "{error:?}");
        assert_eq!(descs.last().unwrap().token, Token::Eos);
        assert!(!descs.iter().any(|d| d.token == Token::Illegal));

        let stream: Vec<Token> = descs.iter().map(|d| d.token).collect();
        assert_eq!(stream[0], Token::Function);
        assert_eq!(stream[1], Token::Identifier);
        assert!(stream.contains(&Token::LessThanEq));
        assert!(stream.contains(&Token::AssignAdd));
        assert!(stream.contains(&Token::String));
        assert!(stream.contains(&Token::Return));
    }

    #[test]
    fn a_number_followed_by_an_identifier_is_rejected() {
        let (_, error) = Scanner::tokenize_all("3in x");
        assert!(error.is_some());
        assert!(error.unwrap().message.contains("identifier starts"));
    }

    #[test]
    fn unimplemented_syntax_reports_an_error_rather_than_mis_scanning() {
        let (_, error) = Scanner::tokenize_all("`template`");
        assert!(error.is_some());
        assert!(error.unwrap().message.contains("template literals"));

        let (_, error) = Scanner::tokenize_all("a @ b");
        assert!(error.is_some());
        assert!(error.unwrap().message.contains("unexpected character"));
    }
}
