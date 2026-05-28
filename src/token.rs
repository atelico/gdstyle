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
