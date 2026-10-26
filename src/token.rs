/// Represents a position in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub length: usize,
}

impl Span {
    pub fn new(line: usize, column: usize, offset: usize, length: usize) -> Self {
        Self {
            line,
            column,
            offset,
            length,
        }
    }
}

/// All possible token types in GDScript.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Integer(i64),
    Float(f64),
    String(StringInfo),
    Bool(bool),
    Null,

    // Identifiers and keywords
    Identifier(String),
    ClassName,
    Extends,
    Class,
    Func,
    Var,
    Const,
    Signal,
    Enum,
    Static,
    If,
    Elif,
    Else,
    For,
    While,
    Match,
    When,
    Break,
    Continue,
    Pass,
    Return,
    As,
    Is,
    In,
    Not,
    And,
    Or,
    Self_,
    Super,
    Await,
    Assert,
    Breakpoint,
    Preload,
    Void,
    Trait,

    // Annotations
    Annotation(String),

    // Operators
    Plus,
    Minus,
    Star,
    StarStar,
    Slash,
    Percent,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    StarStarAssign,
    LessLessAssign,
    GreaterGreaterAssign,
    AmpersandAssign,
    PipeAssign,
    CaretAssign,
    Ampersand,
    Pipe,
    Caret,
    Tilde,
    LessLess,
    GreaterGreater,
    Arrow,
    AmpersandAmpersand,
    PipePipe,
    Bang,

    // Delimiters
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Colon,
    Semicolon,
    Dot,
    DotDot,
    Ellipsis,
    /// `$`: the `get_node` shorthand prefix (`$Player`).
    Dollar,
    /// `%`: the unique-node-name access prefix (`%HealthBar`). Distinct from
    /// the `Percent` modulo operator.
    UniqueNodeMarker,

    // Structure
    Newline,
    Indent,
    Dedent,

    // Comments
    Comment(String),
    DocComment(String),

    // Special
    Eof,
    Error(String),
}

/// Metadata about a string literal.
#[derive(Debug, Clone, PartialEq)]
pub struct StringInfo {
    pub value: String,
    pub quote_style: QuoteStyle,
    pub prefix: StringPrefix,
    pub is_multiline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteStyle {
    Single,
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringPrefix {
    None,
    Raw,
    StringName,
    NodePath,
}

/// A single token with its span and raw text.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span, text: String) -> Self {
        Self { kind, span, text }
    }
}

/// The word that replaces the `&&`, `||` or `!` token at `idx`, with a space
/// added where the token touches its neighbour, so the word cannot join an
/// identifier: `a&&b||!c` becomes `a and b or not c`, not `aandbornot c`.
/// Returns `None` when the token at `idx` is not one of those operators.
///
/// # Example
///
/// ```
/// use gdstyle::lexer::Lexer;
/// use gdstyle::token::{boolean_operator_word, TokenKind};
///
/// let tokens = Lexer::new("x = a&&b or (!c)").tokenize();
/// let index_of = |kind: TokenKind| tokens.iter().position(|t| t.kind == kind).unwrap();
/// let and_word = boolean_operator_word(&tokens, index_of(TokenKind::AmpersandAmpersand));
/// assert_eq!(and_word.as_deref(), Some(" and "));
/// // `not` after `(` needs no leading space.
/// assert_eq!(boolean_operator_word(&tokens, index_of(TokenKind::Bang)).as_deref(), Some("not "));
/// // Any other token yields `None`.
/// assert_eq!(boolean_operator_word(&tokens, 0), None);
/// ```
pub fn boolean_operator_word(tokens: &[Token], idx: usize) -> Option<String> {
    let token = tokens.get(idx)?;
    let word = match token.kind {
        TokenKind::AmpersandAmpersand => "and",
        TokenKind::PipePipe => "or",
        TokenKind::Bang => "not",
        _ => return None,
    };
    let start = token.span.offset;
    let end = start + token.span.length;
    let before = idx
        .checked_sub(1)
        .and_then(|i| tokens.get(i))
        .filter(|prev| prev.span.offset + prev.span.length == start)
        .and_then(|prev| prev.text.chars().next_back());
    let after = tokens
        .get(idx + 1)
        .filter(|next| next.span.offset == end)
        .and_then(|next| next.text.chars().next());
    // `not` is a prefix: `(!a)` becomes `(not a)`, but `if!a` needs the space.
    let space_before = match token.kind {
        TokenKind::Bang => before.is_some_and(|c| c.is_alphanumeric() || c == '_'),
        _ => before.is_some_and(|c| !c.is_whitespace()),
    };
    let space_after = after.is_some_and(|c| !c.is_whitespace());
    Some(format!(
        "{}{word}{}",
        if space_before { " " } else { "" },
        if space_after { " " } else { "" }
    ))
}

/// The lines of a file that multi-line string literals cover. Layout passes
/// and rules use it to leave string data alone: whitespace, blank lines and
/// line length inside a literal are part of its value.
///
/// # Example
///
/// ```
/// use gdstyle::lexer::Lexer;
/// use gdstyle::token::StringLines;
///
/// let source = "var a = 1\nvar s = \"\"\"\nx\n\"\"\"\nvar b = 2\n";
/// let lines = StringLines::new(&Lexer::new(source).tokenize(), source.split('\n').count());
/// assert!(!lines.is_interior(1) && lines.ends_inside(1)); // `var s = """`
/// assert!(lines.is_interior(2) && lines.ends_inside(2)); // `x`
/// assert!(lines.is_interior(3) && !lines.ends_inside(3)); // `"""`
/// assert_eq!(lines.literal_end(3), Some(3)); // just past the closing `"""`
/// assert!(!lines.is_interior(4) && !lines.ends_inside(4)); // `var b = 2`
/// ```
pub struct StringLines {
    interior: Vec<bool>,
    ends_inside: Vec<bool>,
    literal_end: Vec<Option<usize>>,
}

impl StringLines {
    /// Build the masks for `line_count` lines from the file's tokens.
    pub fn new(tokens: &[Token], line_count: usize) -> Self {
        let mut interior = vec![false; line_count];
        let mut ends_inside = vec![false; line_count];
        let mut literal_end = vec![None; line_count];
        for token in tokens {
            if !matches!(token.kind, TokenKind::String(_)) {
                continue;
            }
            let first = token.span.line;
            let last = first + token.text.matches('\n').count();
            if last > first {
                if let Some(slot) = last.checked_sub(1).and_then(|idx| literal_end.get_mut(idx)) {
                    // The closing line starts with the literal's last segment.
                    *slot = token.text.rsplit('\n').next().map(str::len);
                }
            }
            for line in first..=last {
                let Some(idx) = line.checked_sub(1).filter(|&idx| idx < line_count) else {
                    break;
                };
                // OR, not assign: a later string on the same line (`""" % "x"`)
                // must not clear what the literal before it set.
                interior[idx] |= line > first;
                ends_inside[idx] |= line < last;
            }
        }
        Self {
            interior,
            ends_inside,
            literal_end,
        }
    }

    /// Line `idx` (0-based) starts inside a literal: it comes after the line
    /// that opens one, up to and including the line that closes it. The
    /// closing line may still carry code after the `"""`.
    pub fn is_interior(&self, idx: usize) -> bool {
        self.interior.get(idx).copied().unwrap_or(false)
    }

    /// The end of line `idx` (0-based) is inside a literal: the opening line
    /// and every following line before the one that closes it.
    pub fn ends_inside(&self, idx: usize) -> bool {
        self.ends_inside.get(idx).copied().unwrap_or(false)
    }

    /// When a literal that spans lines closes on line `idx` (0-based), the
    /// byte offset in that line just past its closing quote. Everything
    /// before it is string data; anything after it is code.
    pub fn literal_end(&self, idx: usize) -> Option<usize> {
        self.literal_end.get(idx).copied().flatten()
    }
}
