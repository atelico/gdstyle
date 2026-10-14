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
/// assert!(!lines.is_interior(4) && !lines.ends_inside(4)); // `var b = 2`
/// ```
pub struct StringLines {
    interior: Vec<bool>,
    ends_inside: Vec<bool>,
}

impl StringLines {
    /// Build the masks for `line_count` lines from the file's tokens.
    pub fn new(tokens: &[Token], line_count: usize) -> Self {
        let mut interior = vec![false; line_count];
        let mut ends_inside = vec![false; line_count];
        for token in tokens {
            if !matches!(token.kind, TokenKind::String(_)) {
                continue;
            }
            let first = token.span.line;
            let last = first + token.text.matches('\n').count();
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
}
