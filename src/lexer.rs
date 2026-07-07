use crate::token::*;

/// Tokenizes GDScript source code into a stream of tokens.
pub struct Lexer<'a> {
    source: &'a str,
    chars: Vec<char>,
    position: usize,
    /// Byte offset into `source`: tracks actual byte position for Span offsets.
    byte_offset: usize,
    line: usize,
    column: usize,
    indent_stack: Vec<usize>,
    pending_tokens: Vec<Token>,
    at_line_start: bool,
    paren_depth: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.chars().collect(),
            position: 0,
            byte_offset: 0,
            line: 1,
            column: 1,
            indent_stack: vec![0],
            pending_tokens: Vec::new(),
            at_line_start: true,
            paren_depth: 0,
        }
    }

    /// Tokenize the entire source into a vector of tokens.
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        loop {
            let token = self.next_token();
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        tokens
    }

    fn next_token(&mut self) -> Token {
        // Return pending tokens first (indentation changes).
        if let Some(token) = self.pending_tokens.pop() {
            return token;
        }

        // Handle indentation at line start.
        if self.at_line_start {
            if self.paren_depth == 0 {
                self.at_line_start = false;
                if let Some(indent_tokens) = self.process_indentation() {
                    return indent_tokens;
                }
            } else {
                // Inside brackets/parens/braces, skip leading whitespace but
                // don't track indentation. Reset the flag so we don't re-enter.
                self.at_line_start = false;
                self.skip_line_whitespace();
            }
        }

        self.skip_spaces();

        if self.is_at_end() {
            // Emit remaining dedents before EOF.
            if self.indent_stack.len() > 1 {
                let span = self.current_span(0);
                self.indent_stack.pop();
                return Token::new(TokenKind::Dedent, span, String::new());
            }
            let span = self.current_span(0);
            return Token::new(TokenKind::Eof, span, String::new());
        }

        let ch = self.current_char();

        // Line continuation with backslash.
        if ch == '\\' && self.peek_char() == Some('\n') {
            self.advance(); // skip backslash
            self.advance(); // skip newline
            self.line += 1;
            self.column = 1;
            return self.next_token();
        }

        // Carriage return: skip it (handle \r\n as just \n).
        if ch == '\r' {
            self.advance();
            return self.next_token();
        }

        // Newline.
        if ch == '\n' {
            return self.read_newline();
        }

        // Comments.
        if ch == '#' {
            return self.read_comment();
        }

        // Annotations.
        if ch == '@' {
            return self.read_annotation();
        }

        // Strings.
        if ch == '"' || ch == '\'' {
            return self.read_string(StringPrefix::None);
        }

        // String prefixes.
        if (ch == 'r' || ch == 'R') && self.peek_is_quote() {
            self.advance();
            return self.read_string(StringPrefix::Raw);
        }
        if ch == '&' && self.peek_is_quote() {
            self.advance();
            return self.read_string(StringPrefix::StringName);
        }
        if ch == '^' && self.peek_is_quote() {
            self.advance();
            return self.read_string(StringPrefix::NodePath);
        }

        // Numbers.
        if ch.is_ascii_digit() {
            return self.read_number();
        }

        // Identifiers and keywords.
        if ch.is_alphabetic() || ch == '_' {
            return self.read_identifier();
        }

        // Dollar sign (node path shorthand).
        if ch == '$' {
            return self.read_single_char(TokenKind::Dollar);
        }

        // Operators and delimiters.
        self.read_operator()
    }

    /// Scan ahead from the current position (which must be on a `#` of a
    /// comment line) to the next REAL line — i.e. the next line that is
    /// neither blank nor another comment. Returns that line's leading
    /// indent measured the same way as `process_indentation` (tabs count
    /// as `TAB_WIDTH` columns). Returns `None` if EOF is reached without
    /// finding a real line. Read-only: never mutates lexer state.
    fn peek_next_real_line_indent(&self) -> Option<usize> {
        const TAB_WIDTH: usize = 4;
        let mut i = self.position;
        // Skip to end of current line.
        while i < self.chars.len() && self.chars[i] != '\n' {
            i += 1;
        }
        loop {
            if i >= self.chars.len() {
                return None;
            }
            i += 1; // step past '\n'
                    // Measure leading whitespace of this line.
            let mut indent = 0;
            while i < self.chars.len() {
                match self.chars[i] {
                    '\t' => {
                        indent = (indent / TAB_WIDTH + 1) * TAB_WIDTH;
                        i += 1;
                    }
                    ' ' => {
                        indent += 1;
                        i += 1;
                    }
                    _ => break,
                }
            }
            if i >= self.chars.len() {
                return None;
            }
            match self.chars[i] {
                '\n' => continue, // blank line, look further
                '#' => {
                    // Another comment line: skip to end and keep scanning.
                    while i < self.chars.len() && self.chars[i] != '\n' {
                        i += 1;
                    }
                }
                _ => return Some(indent),
            }
        }
    }

    fn process_indentation(&mut self) -> Option<Token> {
        // Indentation is measured in visual columns: a tab advances to the
        // next multiple of TAB_WIDTH, a space adds one. Counting both as a
        // flat +1 would make one tab and one space compare equal, which
        // produces wrong Indent/Dedent tokens on mixed-indentation files.
        const TAB_WIDTH: usize = 4;
        let mut indent_level = 0;
        let start = self.byte_offset;

        while !self.is_at_end() {
            match self.current_char() {
                '\t' => {
                    indent_level = (indent_level / TAB_WIDTH + 1) * TAB_WIDTH;
                    self.advance();
                }
                ' ' => {
                    indent_level += 1;
                    self.advance();
                }
                '\n' => {
                    // Blank line, skip it.
                    return None;
                }
                '#' => {
                    // Comment-only line. Three cases by relative indent:
                    //
                    // == current: a trailing comment inside an open body.
                    //   Preserve the stack so it stays inside the body.
                    //
                    // <  current: might be a block boundary (top-level
                    //   `## doc` between two functions; `#` comment
                    //   between inner-class methods) or mid-body noise (a
                    //   stray col-1 comment sandwiched between two
                    //   deeper-indented body statements). The two look
                    //   identical on the comment line itself — peek at
                    //   the next REAL line. Deeper → mid-body noise,
                    //   preserve stack. Otherwise → boundary, dedent.
                    //
                    // >  current: the comment is the FIRST line of a new
                    //   indented block (e.g. `class Bar:\n\t## doc\n\tvar
                    //   a: int`). Fall through to Indent emission so the
                    //   block opens here rather than at the comment's
                    //   following code line. Without this, the parser's
                    //   `parse_indented_block` saw no Indent yet, bailed
                    //   with an empty body, and let the `var` fall back
                    //   to the outer scope — which made the ordering rule
                    //   flag every inner-class member.
                    let current_indent = *self.indent_stack.last().unwrap();
                    if indent_level == current_indent {
                        return None;
                    }
                    if indent_level < current_indent
                        && self
                            .peek_next_real_line_indent()
                            .is_some_and(|next| next > indent_level)
                    {
                        return None;
                    }
                    break;
                }
                _ => break,
            }
        }

        if self.is_at_end() {
            return None;
        }

        let current_indent = *self.indent_stack.last().unwrap();

        if indent_level > current_indent {
            self.indent_stack.push(indent_level);
            let span = Span::new(self.line, 1, start, indent_level);
            return Some(Token::new(TokenKind::Indent, span, String::new()));
        }

        if indent_level < current_indent {
            // Emit dedent tokens.
            let span = Span::new(self.line, 1, start, 0);
            let mut dedent_count = 0;
            while self.indent_stack.len() > 1 && *self.indent_stack.last().unwrap() > indent_level {
                self.indent_stack.pop();
                dedent_count += 1;
            }
            // Queue all but the first dedent as pending.
            for _ in 1..dedent_count {
                self.pending_tokens
                    .push(Token::new(TokenKind::Dedent, span, String::new()));
            }
            if dedent_count > 0 {
                return Some(Token::new(TokenKind::Dedent, span, String::new()));
            }
        }

        None
    }

    fn read_newline(&mut self) -> Token {
        let span = self.current_span(1);
        self.advance();
        self.line += 1;
        self.column = 1;
        self.at_line_start = true;

        // Don't emit newlines inside parentheses/brackets/braces.
        if self.paren_depth > 0 {
            return self.next_token();
        }

        Token::new(TokenKind::Newline, span, "\n".to_string())
    }

    fn read_comment(&mut self) -> Token {
        let start_offset = self.byte_offset;
        let start_col = self.column;
        let mut text = String::new();

        // Check for doc comment (##).
        self.advance(); // skip first #
        text.push('#');

        if !self.is_at_end() && self.current_char() == '#' {
            self.advance(); // skip second #
            text.push('#');
            // Read rest of line for doc comment.
            while !self.is_at_end() && self.current_char() != '\n' {
                text.push(self.current_char());
                self.advance();
            }
            let content = text[2..].to_string();
            let span = Span::new(self.line, start_col, start_offset, text.len());
            return Token::new(TokenKind::DocComment(content), span, text);
        }

        // Regular comment.
        while !self.is_at_end() && self.current_char() != '\n' {
            text.push(self.current_char());
            self.advance();
        }
        let content = text[1..].to_string();
        let span = Span::new(self.line, start_col, start_offset, text.len());
        Token::new(TokenKind::Comment(content), span, text)
    }

    fn read_annotation(&mut self) -> Token {
        let start_offset = self.byte_offset;
        let start_col = self.column;

        self.advance(); // skip @

        let mut name = String::new();
        while !self.is_at_end()
            && (self.current_char().is_ascii_alphanumeric() || self.current_char() == '_')
        {
            name.push(self.current_char());
            self.advance();
        }

        let text = format!("@{}", name);
        let span = Span::new(self.line, start_col, start_offset, text.len());
        Token::new(TokenKind::Annotation(name), span, text)
    }

    fn read_string(&mut self, prefix: StringPrefix) -> Token {
        let start_offset = self.byte_offset
            - match prefix {
                StringPrefix::None => 0,
                _ => 1,
            };
        let start_col = self.column
            - match prefix {
                StringPrefix::None => 0,
                _ => 1,
            };
        let start_line = self.line;

        let quote = self.current_char();
        let quote_style = match quote {
            '\'' => QuoteStyle::Single,
            _ => QuoteStyle::Double,
        };

        self.advance(); // skip opening quote

        // Check for triple-quoted string.
        let is_multiline =
            !self.is_at_end() && self.current_char() == quote && self.peek_char() == Some(quote);

        if is_multiline {
            self.advance(); // skip second quote
            self.advance(); // skip third quote
        }

        let mut value = String::new();
        let mut closed = false;

        while !self.is_at_end() {
            let ch = self.current_char();

            if ch == '\n' {
                // GDScript 4 allows a literal newline inside a regular
                // single- or double-quoted string, not just triple-quoted
                // ones: the string runs to its closing quote. A genuinely
                // unterminated string is still caught: `closed` stays false
                // if we hit EOF without the quote.
                value.push(ch);
                self.advance();
                self.line += 1;
                self.column = 1;
                continue;
            }

            if ch == '\\' && prefix != StringPrefix::Raw {
                value.push(ch);
                self.advance();
                if !self.is_at_end() {
                    let escaped = self.current_char();
                    value.push(escaped);
                    self.advance();
                    if escaped == '\n' {
                        self.line += 1;
                        self.column = 1;
                    }
                }
                continue;
            }

            if ch == quote {
                if is_multiline {
                    if self.peek_char() == Some(quote) && self.peek_char_at(2) == Some(quote) {
                        self.advance(); // skip first closing quote
                        self.advance(); // skip second closing quote
                        self.advance(); // skip third closing quote
                        closed = true;
                        break;
                    }
                    value.push(ch);
                    self.advance();
                    continue;
                } else {
                    self.advance(); // skip closing quote
                    closed = true;
                    break;
                }
            }

            value.push(ch);
            self.advance();
        }

        let raw_text = self.source[start_offset..self.byte_offset].to_string();

        if !closed {
            let span = Span::new(start_line, start_col, start_offset, raw_text.len());
            return Token::new(
                TokenKind::Error("unterminated string".to_string()),
                span,
                raw_text,
            );
        }

        let span = Span::new(start_line, start_col, start_offset, raw_text.len());
        Token::new(
            TokenKind::String(StringInfo {
                value,
                quote_style,
                prefix,
                is_multiline,
            }),
            span,
            raw_text,
        )
    }

    fn read_number(&mut self) -> Token {
        let start_offset = self.byte_offset;
        let start_col = self.column;
        let mut text = String::new();

        let first = self.current_char();
        text.push(first);
        self.advance();

        // Check for hex (0x), binary (0b).
        if first == '0' && !self.is_at_end() {
            let next = self.current_char();
            if next == 'x' || next == 'X' {
                text.push(next);
                self.advance();
                return self.read_hex_number(start_offset, start_col, text);
            }
            if next == 'b' || next == 'B' {
                text.push(next);
                self.advance();
                return self.read_binary_number(start_offset, start_col, text);
            }
        }

        // Read decimal digits.
        while !self.is_at_end()
            && (self.current_char().is_ascii_digit() || self.current_char() == '_')
        {
            text.push(self.current_char());
            self.advance();
        }

        // Check for float.
        let mut is_float = false;
        if !self.is_at_end() && self.current_char() == '.' {
            // Make sure it's not a range operator (..) or method call.
            if self.peek_char() != Some('.') {
                is_float = true;
                text.push('.');
                self.advance();
                while !self.is_at_end()
                    && (self.current_char().is_ascii_digit() || self.current_char() == '_')
                {
                    text.push(self.current_char());
                    self.advance();
                }
            }
        }

        // Check for exponent.
        if !self.is_at_end() && (self.current_char() == 'e' || self.current_char() == 'E') {
            is_float = true;
            text.push(self.current_char());
            self.advance();
            if !self.is_at_end() && (self.current_char() == '+' || self.current_char() == '-') {
                text.push(self.current_char());
                self.advance();
            }
            while !self.is_at_end()
                && (self.current_char().is_ascii_digit() || self.current_char() == '_')
            {
                text.push(self.current_char());
                self.advance();
            }
        }

        let span = Span::new(self.line, start_col, start_offset, text.len());
        let clean = text.replace('_', "");

        if is_float {
            match clean.parse::<f64>() {
                Ok(val) => Token::new(TokenKind::Float(val), span, text),
                Err(_) => Token::new(
                    TokenKind::Error(format!("invalid float: {}", text)),
                    span,
                    text,
                ),
            }
        } else {
            match clean.parse::<i64>() {
                Ok(val) => Token::new(TokenKind::Integer(val), span, text),
                Err(_) => Token::new(
                    TokenKind::Error(format!("invalid integer: {}", text)),
                    span,
                    text,
                ),
            }
        }
    }

    fn read_hex_number(
        &mut self,
        start_offset: usize,
        start_col: usize,
        mut text: String,
    ) -> Token {
        while !self.is_at_end()
            && (self.current_char().is_ascii_hexdigit() || self.current_char() == '_')
        {
            text.push(self.current_char());
            self.advance();
        }

        let span = Span::new(self.line, start_col, start_offset, text.len());
        let hex_str = text[2..].replace('_', "");
        match i64::from_str_radix(&hex_str, 16) {
            Ok(val) => Token::new(TokenKind::Integer(val), span, text),
            Err(_) => Token::new(
                TokenKind::Error(format!("invalid hex: {}", text)),
                span,
                text,
            ),
        }
    }

    fn read_binary_number(
        &mut self,
        start_offset: usize,
        start_col: usize,
        mut text: String,
    ) -> Token {
        while !self.is_at_end()
            && (self.current_char() == '0'
                || self.current_char() == '1'
                || self.current_char() == '_')
        {
            text.push(self.current_char());
            self.advance();
        }

        let span = Span::new(self.line, start_col, start_offset, text.len());
        let bin_str = text[2..].replace('_', "");
        match i64::from_str_radix(&bin_str, 2) {
            Ok(val) => Token::new(TokenKind::Integer(val), span, text),
            Err(_) => Token::new(
                TokenKind::Error(format!("invalid binary: {}", text)),
                span,
                text,
            ),
        }
    }

    fn read_identifier(&mut self) -> Token {
        let start_offset = self.byte_offset;
        let start_col = self.column;
        let mut text = String::new();

        while !self.is_at_end()
            && (self.current_char().is_alphanumeric() || self.current_char() == '_')
        {
            text.push(self.current_char());
            self.advance();
        }

        let span = Span::new(self.line, start_col, start_offset, text.len());
        let kind = match text.as_str() {
            "class_name" => TokenKind::ClassName,
            "extends" => TokenKind::Extends,
            "class" => TokenKind::Class,
            "func" => TokenKind::Func,
            "var" => TokenKind::Var,
            "const" => TokenKind::Const,
            "signal" => TokenKind::Signal,
            "enum" => TokenKind::Enum,
            "static" => TokenKind::Static,
            "if" => TokenKind::If,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "for" => TokenKind::For,
            "while" => TokenKind::While,
            "match" => TokenKind::Match,
            "when" => TokenKind::When,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "pass" => TokenKind::Pass,
            "return" => TokenKind::Return,
            "as" => TokenKind::As,
            "is" => TokenKind::Is,
            "in" => TokenKind::In,
            "not" => TokenKind::Not,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "self" => TokenKind::Self_,
            "super" => TokenKind::Super,
            "await" => TokenKind::Await,
            "assert" => TokenKind::Assert,
            "breakpoint" => TokenKind::Breakpoint,
            "preload" => TokenKind::Preload,
            "void" => TokenKind::Void,
            "trait" => TokenKind::Trait,
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            "null" => TokenKind::Null,
            "PI" => TokenKind::Identifier(text.clone()),
            "TAU" => TokenKind::Identifier(text.clone()),
            "INF" => TokenKind::Identifier(text.clone()),
            "NaN" => TokenKind::Identifier(text.clone()),
            _ => TokenKind::Identifier(text.clone()),
        };

        Token::new(kind, span, text)
    }

    fn read_operator(&mut self) -> Token {
        let start_offset = self.byte_offset;
        let start_col = self.column;
        let ch = self.current_char();
        self.advance();

        // Unexpected character: build an owned Error token directly. The match
        // below yields `&'static str` text for the known operators; an
        // unexpected char has no static text, so we handle it here rather
        // than leaking a heap allocation to satisfy the type.
        if !is_known_operator_start(ch) {
            let text = ch.to_string();
            let span = Span::new(self.line, start_col, start_offset, text.len());
            return Token::new(
                TokenKind::Error(format!("unexpected character: {}", ch)),
                span,
                text,
            );
        }

        let (kind, text) = match ch {
            '(' => {
                self.paren_depth += 1;
                (TokenKind::LeftParen, "(")
            }
            ')' => {
                if self.paren_depth > 0 {
                    self.paren_depth -= 1;
                }
                (TokenKind::RightParen, ")")
            }
            '[' => {
                self.paren_depth += 1;
                (TokenKind::LeftBracket, "[")
            }
            ']' => {
                if self.paren_depth > 0 {
                    self.paren_depth -= 1;
                }
                (TokenKind::RightBracket, "]")
            }
            '{' => {
                self.paren_depth += 1;
                (TokenKind::LeftBrace, "{")
            }
            '}' => {
                if self.paren_depth > 0 {
                    self.paren_depth -= 1;
                }
                (TokenKind::RightBrace, "}")
            }
            ',' => (TokenKind::Comma, ","),
            ';' => (TokenKind::Semicolon, ";"),
            ':' => (TokenKind::Colon, ":"),
            '~' => (TokenKind::Tilde, "~"),
            '+' => {
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::PlusAssign, "+=")
                } else {
                    (TokenKind::Plus, "+")
                }
            }
            '-' => {
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::MinusAssign, "-=")
                } else if !self.is_at_end() && self.current_char() == '>' {
                    self.advance();
                    (TokenKind::Arrow, "->")
                } else {
                    (TokenKind::Minus, "-")
                }
            }
            '*' => {
                if !self.is_at_end() && self.current_char() == '*' {
                    self.advance();
                    if !self.is_at_end() && self.current_char() == '=' {
                        self.advance();
                        (TokenKind::StarStarAssign, "**=")
                    } else {
                        (TokenKind::StarStar, "**")
                    }
                } else if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::StarAssign, "*=")
                } else {
                    (TokenKind::Star, "*")
                }
            }
            '/' => {
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::SlashAssign, "/=")
                } else {
                    (TokenKind::Slash, "/")
                }
            }
            '%' => {
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::PercentAssign, "%=")
                } else if !self.is_at_end()
                    && (self.current_char().is_ascii_alphabetic()
                        || self.current_char() == '_'
                        || self.current_char() == '"')
                {
                    // Unique node shorthand like %Player or %"Node Name"
                    (TokenKind::UniqueNodeMarker, "%")
                } else {
                    (TokenKind::Percent, "%")
                }
            }
            '=' => {
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::Equal, "==")
                } else {
                    (TokenKind::Assign, "=")
                }
            }
            '!' => {
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::NotEqual, "!=")
                } else {
                    (TokenKind::Bang, "!")
                }
            }
            '<' => {
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::LessEqual, "<=")
                } else if !self.is_at_end() && self.current_char() == '<' {
                    self.advance();
                    if !self.is_at_end() && self.current_char() == '=' {
                        self.advance();
                        (TokenKind::LessLessAssign, "<<=")
                    } else {
                        (TokenKind::LessLess, "<<")
                    }
                } else {
                    (TokenKind::Less, "<")
                }
            }
            '>' => {
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::GreaterEqual, ">=")
                } else if !self.is_at_end() && self.current_char() == '>' {
                    self.advance();
                    if !self.is_at_end() && self.current_char() == '=' {
                        self.advance();
                        (TokenKind::GreaterGreaterAssign, ">>=")
                    } else {
                        (TokenKind::GreaterGreater, ">>")
                    }
                } else {
                    (TokenKind::Greater, ">")
                }
            }
            '&' => {
                if !self.is_at_end() && self.current_char() == '&' {
                    self.advance();
                    (TokenKind::AmpersandAmpersand, "&&")
                } else if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::AmpersandAssign, "&=")
                } else {
                    (TokenKind::Ampersand, "&")
                }
            }
            '|' => {
                if !self.is_at_end() && self.current_char() == '|' {
                    self.advance();
                    (TokenKind::PipePipe, "||")
                } else if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::PipeAssign, "|=")
                } else {
                    (TokenKind::Pipe, "|")
                }
            }
            '^' => {
                if !self.is_at_end() && self.current_char() == '=' {
                    self.advance();
                    (TokenKind::CaretAssign, "^=")
                } else {
                    (TokenKind::Caret, "^")
                }
            }
            '.' => {
                if !self.is_at_end() && self.current_char() == '.' {
                    self.advance();
                    if !self.is_at_end() && self.current_char() == '.' {
                        self.advance();
                        (TokenKind::Ellipsis, "...")
                    } else {
                        (TokenKind::DotDot, "..")
                    }
                } else {
                    (TokenKind::Dot, ".")
                }
            }
            // `is_known_operator_start` guarantees `ch` is one of the arms
            // above, so this is genuinely unreachable.
            _ => unreachable!("unexpected operator char {:?} passed the guard", ch),
        };

        let len = text.len();
        let span = Span::new(self.line, start_col, start_offset, len);
        Token::new(kind, span, text.to_string())
    }

    fn read_single_char(&mut self, kind: TokenKind) -> Token {
        let span = self.current_span(1);
        let text = self.current_char().to_string();
        self.advance();
        Token::new(kind, span, text)
    }

    // Helper methods.

    fn current_char(&self) -> char {
        self.chars[self.position]
    }

    fn peek_char(&self) -> Option<char> {
        self.chars.get(self.position + 1).copied()
    }

    fn peek_char_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.position + offset).copied()
    }

    fn peek_is_quote(&self) -> bool {
        matches!(self.peek_char(), Some('"') | Some('\''))
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.chars.len()
    }

    fn advance(&mut self) {
        if self.position < self.chars.len() {
            self.byte_offset += self.chars[self.position].len_utf8();
        }
        self.position += 1;
        self.column += 1;
    }

    /// Skip all leading whitespace (tabs and spaces) on a line inside brackets.
    fn skip_line_whitespace(&mut self) {
        while !self.is_at_end() && (self.current_char() == ' ' || self.current_char() == '\t') {
            self.advance();
        }
    }

    fn skip_spaces(&mut self) {
        while !self.is_at_end() && self.current_char() == ' ' {
            self.advance();
        }
        // Also skip tabs if not at line start (mid-line tabs are treated as spaces).
        while !self.is_at_end() && self.current_char() == '\t' && !self.at_line_start {
            self.advance();
        }
    }

    fn current_span(&self, length: usize) -> Span {
        Span::new(self.line, self.column, self.byte_offset, length)
    }
}

/// True if `ch` begins one of the operator / delimiter tokens `read_operator`
/// recognises. Anything else is lexed as a `TokenKind::Error`.
fn is_known_operator_start(ch: char) -> bool {
    matches!(
        ch,
        '-' | ','
            | ';'
            | ':'
            | '!'
            | '.'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '*'
            | '/'
            | '&'
            | '%'
            | '^'
            | '+'
            | '<'
            | '='
            | '>'
            | '|'
            | '~'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(source: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(source);
        lexer.tokenize()
    }

    fn token_kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source)
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !matches!(k, TokenKind::Newline | TokenKind::Eof))
            .collect()
    }

    #[test]
    fn empty_source() {
        let tokens = tokenize("");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn simple_integer() {
        let kinds = token_kinds("42");
        assert_eq!(kinds, vec![TokenKind::Integer(42)]);
    }

    #[test]
    fn hex_number() {
        let kinds = token_kinds("0xff");
        assert_eq!(kinds, vec![TokenKind::Integer(255)]);
    }

    #[test]
    fn binary_number() {
        let kinds = token_kinds("0b1010");
        assert_eq!(kinds, vec![TokenKind::Integer(10)]);
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn float_number() {
        let kinds = token_kinds("3.14");
        assert_eq!(kinds, vec![TokenKind::Float(3.14)]);
    }

    #[test]
    fn number_with_underscores() {
        let kinds = token_kinds("1_000_000");
        assert_eq!(kinds, vec![TokenKind::Integer(1_000_000)]);
    }

    #[test]
    fn scientific_notation() {
        let kinds = token_kinds("1e10");
        assert_eq!(kinds, vec![TokenKind::Float(1e10)]);
    }

    #[test]
    fn double_quoted_string() {
        let kinds = token_kinds(r#""hello""#);
        assert!(
            matches!(kinds[0], TokenKind::String(ref s) if s.value == "hello" && s.quote_style == QuoteStyle::Double)
        );
    }

    #[test]
    fn single_quoted_string() {
        let kinds = token_kinds("'world'");
        assert!(
            matches!(kinds[0], TokenKind::String(ref s) if s.value == "world" && s.quote_style == QuoteStyle::Single)
        );
    }

    #[test]
    fn string_name_literal() {
        let kinds = token_kinds("&\"MySignal\"");
        assert!(
            matches!(kinds[0], TokenKind::String(ref s) if s.prefix == StringPrefix::StringName)
        );
    }

    #[test]
    fn node_path_literal() {
        let kinds = token_kinds("^\"UI/HealthBar\"");
        assert!(matches!(kinds[0], TokenKind::String(ref s) if s.prefix == StringPrefix::NodePath));
    }

    #[test]
    fn triple_quoted_string() {
        let source = r#""""hello
world""""#;
        let kinds = token_kinds(source);
        assert!(matches!(kinds[0], TokenKind::String(ref s) if s.is_multiline));
    }

    #[test]
    fn keywords() {
        let kinds = token_kinds("func var const if else for while return");
        assert_eq!(
            kinds,
            vec![
                TokenKind::Func,
                TokenKind::Var,
                TokenKind::Const,
                TokenKind::If,
                TokenKind::Else,
                TokenKind::For,
                TokenKind::While,
                TokenKind::Return,
            ]
        );
    }

    #[test]
    fn boolean_literals() {
        let kinds = token_kinds("true false");
        assert_eq!(kinds, vec![TokenKind::Bool(true), TokenKind::Bool(false)]);
    }

    #[test]
    fn null_literal() {
        let kinds = token_kinds("null");
        assert_eq!(kinds, vec![TokenKind::Null]);
    }

    #[test]
    fn identifiers() {
        let kinds = token_kinds("player_speed maxHP _private");
        assert_eq!(
            kinds,
            vec![
                TokenKind::Identifier("player_speed".to_string()),
                TokenKind::Identifier("maxHP".to_string()),
                TokenKind::Identifier("_private".to_string()),
            ]
        );
    }

    #[test]
    fn annotation() {
        let kinds = token_kinds("@export");
        assert_eq!(kinds, vec![TokenKind::Annotation("export".to_string())]);
    }

    #[test]
    fn export_annotations() {
        let kinds = token_kinds("@export_range @onready @tool");
        assert_eq!(
            kinds,
            vec![
                TokenKind::Annotation("export_range".to_string()),
                TokenKind::Annotation("onready".to_string()),
                TokenKind::Annotation("tool".to_string()),
            ]
        );
    }

    #[test]
    fn operators() {
        let kinds = token_kinds("+ - * / % ** == != <= >= = += -> && ||");
        assert_eq!(
            kinds,
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::StarStar,
                TokenKind::Equal,
                TokenKind::NotEqual,
                TokenKind::LessEqual,
                TokenKind::GreaterEqual,
                TokenKind::Assign,
                TokenKind::PlusAssign,
                TokenKind::Arrow,
                TokenKind::AmpersandAmpersand,
                TokenKind::PipePipe,
            ]
        );
    }

    #[test]
    fn delimiters() {
        let kinds = token_kinds("( ) [ ] { } , : ;");
        assert_eq!(
            kinds,
            vec![
                TokenKind::LeftParen,
                TokenKind::RightParen,
                TokenKind::LeftBracket,
                TokenKind::RightBracket,
                TokenKind::LeftBrace,
                TokenKind::RightBrace,
                TokenKind::Comma,
                TokenKind::Colon,
                TokenKind::Semicolon,
            ]
        );
    }

    #[test]
    fn dots() {
        let kinds = token_kinds(". .. ...");
        assert_eq!(
            kinds,
            vec![TokenKind::Dot, TokenKind::DotDot, TokenKind::Ellipsis]
        );
    }

    #[test]
    fn comments() {
        let kinds = token_kinds("# this is a comment");
        assert_eq!(
            kinds,
            vec![TokenKind::Comment(" this is a comment".to_string())]
        );
    }

    #[test]
    fn doc_comments() {
        let kinds = token_kinds("## This is documentation");
        assert_eq!(
            kinds,
            vec![TokenKind::DocComment(" This is documentation".to_string())]
        );
    }

    #[test]
    fn indentation_produces_indent_dedent() {
        let source = "if true:\n\tpass\n";
        let kinds: Vec<TokenKind> = tokenize(source)
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !matches!(k, TokenKind::Eof))
            .collect();
        assert!(kinds.contains(&TokenKind::Indent));
        assert!(kinds.contains(&TokenKind::Dedent));
    }

    #[test]
    fn nested_indentation() {
        let source = "if true:\n\tif false:\n\t\tpass\n";
        let tokens = tokenize(source);
        let indent_count = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Indent)
            .count();
        let dedent_count = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Dedent)
            .count();
        assert_eq!(indent_count, 2);
        assert_eq!(dedent_count, 2);
    }

    #[test]
    fn no_newline_inside_parens() {
        let source = "func foo(\n\tx: int,\n\ty: int\n):\n\tpass\n";
        let tokens = tokenize(source);
        // Inside parens, newlines should be suppressed.
        // The first newline token should only appear after the closing paren+colon.
        let newline_count = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Newline)
            .count();
        // We should get newlines for the ): line and the pass line, but not inside parens.
        assert!(newline_count <= 3);
    }

    #[test]
    fn gdscript_function_declaration() {
        let source = "func take_damage(amount: int) -> void:";
        let kinds = token_kinds(source);
        assert_eq!(
            kinds,
            vec![
                TokenKind::Func,
                TokenKind::Identifier("take_damage".to_string()),
                TokenKind::LeftParen,
                TokenKind::Identifier("amount".to_string()),
                TokenKind::Colon,
                TokenKind::Identifier("int".to_string()),
                TokenKind::RightParen,
                TokenKind::Arrow,
                TokenKind::Void,
                TokenKind::Colon,
            ]
        );
    }

    #[test]
    fn signal_declaration() {
        let source = "signal health_changed(old_value: int, new_value: int)";
        let kinds = token_kinds(source);
        assert_eq!(kinds[0], TokenKind::Signal);
        assert_eq!(
            kinds[1],
            TokenKind::Identifier("health_changed".to_string())
        );
    }

    #[test]
    fn enum_declaration() {
        let source = "enum State { IDLE, WALKING, RUNNING }";
        let kinds = token_kinds(source);
        assert_eq!(kinds[0], TokenKind::Enum);
        assert_eq!(kinds[1], TokenKind::Identifier("State".to_string()));
    }

    #[test]
    fn dollar_sign_node_path() {
        let kinds = token_kinds("$Label");
        assert_eq!(
            kinds,
            vec![
                TokenKind::Dollar,
                TokenKind::Identifier("Label".to_string()),
            ]
        );
    }

    #[test]
    fn boolean_operators() {
        let kinds = token_kinds("and or not");
        assert_eq!(kinds, vec![TokenKind::And, TokenKind::Or, TokenKind::Not]);
    }

    #[test]
    fn raw_string() {
        let kinds = token_kinds("r\"hello\\nworld\"");
        assert!(
            matches!(kinds[0], TokenKind::String(ref s) if s.prefix == StringPrefix::Raw && s.value == "hello\\nworld")
        );
    }

    #[test]
    fn line_continuation() {
        let source = "var x = 1 +\\\n\t2";
        let kinds = token_kinds(source);
        // The backslash-newline should be consumed, so we get a continuous expression.
        assert!(kinds.contains(&TokenKind::Plus));
        assert!(kinds.contains(&TokenKind::Integer(2)));
    }

    #[test]
    fn class_name_keyword() {
        let kinds = token_kinds("class_name Player");
        assert_eq!(
            kinds,
            vec![
                TokenKind::ClassName,
                TokenKind::Identifier("Player".to_string()),
            ]
        );
    }

    #[test]
    fn match_and_when() {
        let kinds = token_kinds("match when");
        assert_eq!(kinds, vec![TokenKind::Match, TokenKind::When]);
    }

    #[test]
    fn regular_string_may_span_lines() {
        // GDScript 4 allows a literal newline in a regular double-quoted
        // string; it is not an unterminated string.
        let tokens = tokenize("var s = \"line one\nline two\"\n");
        assert!(
            !tokens.iter().any(|t| matches!(t.kind, TokenKind::Error(_))),
            "multi-line regular string must not be an Error token: {:?}",
            tokens
        );
        assert!(tokens.iter().any(|t| matches!(
            &t.kind,
            TokenKind::String(info) if info.value == "line one\nline two"
        )));
    }

    #[test]
    fn genuinely_unterminated_string_is_an_error() {
        // A string with no closing quote before EOF is still an error.
        let tokens = tokenize("var s = \"no closing quote\n");
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Error(_))));
    }

    #[test]
    fn escape_sequence_in_string() {
        let kinds = token_kinds(r#""hello\nworld""#);
        assert!(matches!(kinds[0], TokenKind::String(ref s) if s.value.contains("\\n")));
    }

    #[test]
    fn multiline_string_preserves_newlines() {
        let source = "\"\"\"line1\nline2\nline3\"\"\"";
        let kinds = token_kinds(source);
        assert!(matches!(kinds[0], TokenKind::String(ref s) if s.value.contains('\n')));
    }

    #[test]
    fn full_gdscript_snippet() {
        let source = r#"class_name Player
extends CharacterBody2D

@export var speed: float = 200.0

func _ready() -> void:
	pass
"#;
        let tokens = tokenize(source);
        // Should tokenize without errors.
        let errors: Vec<&Token> = tokens
            .iter()
            .filter(|t| matches!(t.kind, TokenKind::Error(_)))
            .collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn doc_comment_at_lower_indent_emits_dedents() {
        // Regression: a `## doc` line at column 1 appearing while the lexer
        // is still inside an indented function body used to be swallowed
        // with no dedent emitted. The parser would then consume the doc as
        // part of the previous function's body, and the formatter would
        // detach it from the function it's actually documenting by
        // inserting the canonical blank-line gap between top-level members.
        let source = "func a():\n\tif true:\n\t\tpass\n## doc for b\nfunc b():\n\tpass\n";
        let tokens = tokenize(source);
        let doc_pos = tokens
            .iter()
            .position(|t| matches!(t.kind, TokenKind::DocComment(_)))
            .expect("DocComment should be emitted");
        let dedents_before_doc = tokens[..doc_pos]
            .iter()
            .filter(|t| t.kind == TokenKind::Dedent)
            .count();
        // Two indents were opened (function body + if body); both must be
        // closed BEFORE the doc comment so the doc lives at top level.
        assert_eq!(
            dedents_before_doc, 2,
            "expected 2 dedents before DocComment, got {dedents_before_doc} (tokens: {tokens:?})"
        );
    }

    #[test]
    fn comment_at_lower_indent_followed_by_deeper_real_line_preserves_block() {
        // Counterpart to `doc_comment_at_lower_indent_emits_dedents`: a
        // col-1 comment sandwiched between two deeper-indented body
        // statements is mid-body noise, not a block boundary. The
        // peek-ahead must recognise that the next real line is deeper
        // than the comment and refuse to dedent — otherwise the
        // function gets split in half and the lines after the comment
        // look orphaned.
        let source = "func a():\n\tif true:\n\t\tvar x = 1\n# col-1 mid-body\n\t\tvar y = 2\n\t\treturn x + y\n";
        let tokens = tokenize(source);
        let comment_pos = tokens
            .iter()
            .position(|t| matches!(t.kind, TokenKind::Comment(_)))
            .expect("Comment should be emitted");
        let dedents_before_comment = tokens[..comment_pos]
            .iter()
            .filter(|t| t.kind == TokenKind::Dedent)
            .count();
        assert_eq!(
            dedents_before_comment, 0,
            "mid-body col-1 comment must not split the enclosing block"
        );
    }

    #[test]
    fn comment_at_deeper_indent_opens_block() {
        // Regression for issue #5: an inner-class body whose first
        // line is a comment used to NOT push an Indent onto the
        // stack (the lexer preserved the indent stack for any
        // comment-only line). The parser's `parse_indented_block`
        // then saw no Indent, bailed with an empty body, and the
        // inner-class members fell back to the outer scope —
        // tripping `order/class-member-order` on every one.
        //
        // The deeper-indent comment must now open the block right
        // away, so the body is parsed inside the inner class.
        let source = "class Bar:\n\t## doc\n\tvar a: int\n";
        let tokens = tokenize(source);
        let var_pos = tokens
            .iter()
            .position(|t| t.kind == TokenKind::Var)
            .expect("Var token should be present");
        let indents_before_var = tokens[..var_pos]
            .iter()
            .filter(|t| t.kind == TokenKind::Indent)
            .count();
        assert_eq!(
            indents_before_var, 1,
            "an Indent must be emitted before the doc comment that opens the inner-class body, got tokens: {:?}",
            tokens
        );
    }

    #[test]
    fn comment_at_same_indent_preserves_block() {
        // Counterpart to the regression above: a comment at the same indent
        // as the current body is part of the body and MUST NOT trigger a
        // dedent (otherwise inline comments would tear functions apart).
        let source = "func a():\n\t# inside\n\tpass\n";
        let tokens = tokenize(source);
        let comment_pos = tokens
            .iter()
            .position(|t| matches!(t.kind, TokenKind::Comment(_)))
            .expect("Comment should be emitted");
        let dedents_before_comment = tokens[..comment_pos]
            .iter()
            .filter(|t| t.kind == TokenKind::Dedent)
            .count();
        assert_eq!(
            dedents_before_comment, 0,
            "in-body comment must not dedent the block"
        );
    }

    #[test]
    fn one_tab_is_not_one_space_of_indent() {
        // Regression: a tab and a space both counted as a flat +1, so a
        // tab-indented body and a one-space-indented body compared equal.
        // A tab now counts as TAB_WIDTH columns.
        let tab_body = tokenize("func f():\n\tpass\n");
        let space_body = tokenize("func f():\n pass\n");
        let indent_span = |toks: &[Token]| {
            toks.iter()
                .find(|t| t.kind == TokenKind::Indent)
                .map(|t| t.span.length)
        };
        assert_eq!(
            indent_span(&tab_body),
            Some(4),
            "tab should count as 4 columns"
        );
        assert_eq!(
            indent_span(&space_body),
            Some(1),
            "space should count as 1 column"
        );
    }
}
