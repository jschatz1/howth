//! Token types for JavaScript/TypeScript/JSX.
//!
//! Based on the ECMAScript specification with TypeScript and JSX extensions.

use crate::span::Span;

/// A token with its kind and source location.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    /// Create a new token.
    #[inline]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// The kind of token.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // === Literals ===
    /// Identifier: `foo`, `_bar`, `$baz`
    Identifier(String),
    /// String literal: `"hello"`, `'world'`
    String(String),
    /// Number literal: `42`, `3.14`, `0xff`
    Number(f64),
    /// BigInt literal: `42n`
    BigInt(String),
    /// Regular expression: `/pattern/flags`
    Regex { pattern: String, flags: String },
    /// Template literal part (no substitutions)
    TemplateNoSub(String),
    /// Template head: `` `hello ${``
    TemplateHead(String),
    /// Template middle: `` } middle ${``
    TemplateMiddle(String),
    /// Template tail: `` } end` ``
    TemplateTail(String),

    // === Keywords ===
    // Declarations
    Var,
    Let,
    Const,
    Function,
    Class,

    // Control flow
    If,
    Else,
    Switch,
    Case,
    Default,
    For,
    While,
    Do,
    Break,
    Continue,
    Return,

    // Exception handling
    Try,
    Catch,
    Finally,
    Throw,

    // Operators as keywords
    New,
    Delete,
    Typeof,
    Void,
    In,
    Instanceof,

    // Values
    This,
    Super,
    Null,
    True,
    False,

    // Modules
    Import,
    Export,
    From,
    As,

    // Async
    Async,
    Await,

    // Generators
    Yield,

    // Class modifiers
    Static,
    Get,
    Set,
    Extends,

    // Other
    With,
    Debugger,

    // TypeScript keywords (when feature enabled)
    #[cfg(feature = "typescript")]
    Type,
    #[cfg(feature = "typescript")]
    Interface,
    #[cfg(feature = "typescript")]
    Enum,
    #[cfg(feature = "typescript")]
    Namespace,
    #[cfg(feature = "typescript")]
    Module,
    #[cfg(feature = "typescript")]
    Declare,
    #[cfg(feature = "typescript")]
    Abstract,
    #[cfg(feature = "typescript")]
    Private,
    #[cfg(feature = "typescript")]
    Protected,
    #[cfg(feature = "typescript")]
    Public,
    #[cfg(feature = "typescript")]
    Readonly,
    #[cfg(feature = "typescript")]
    Override,
    #[cfg(feature = "typescript")]
    Implements,
    #[cfg(feature = "typescript")]
    Is,
    #[cfg(feature = "typescript")]
    Keyof,
    #[cfg(feature = "typescript")]
    Infer,
    #[cfg(feature = "typescript")]
    Never,
    #[cfg(feature = "typescript")]
    Unknown,
    #[cfg(feature = "typescript")]
    Any,
    #[cfg(feature = "typescript")]
    Asserts,
    #[cfg(feature = "typescript")]
    Satisfies,

    // === Punctuation ===
    // Brackets
    LParen,     // (
    RParen,     // )
    LBrace,     // {
    RBrace,     // }
    LBracket,   // [
    RBracket,   // ]

    // Delimiters
    Semicolon,  // ;
    Comma,      // ,
    Colon,      // :
    Dot,        // .
    Question,   // ?
    At,         // @ (decorators)
    Hash,       // # (private fields)

    // Arrows and spreads
    Arrow,      // =>
    Spread,     // ...

    // Optional chaining
    QuestionDot, // ?.

    // === Operators ===
    // Assignment
    Eq,         // =
    PlusEq,     // +=
    MinusEq,    // -=
    StarEq,     // *=
    SlashEq,    // /=
    PercentEq,  // %=
    StarStarEq, // **=
    AmpEq,      // &=
    PipeEq,     // |=
    CaretEq,    // ^=
    LtLtEq,     // <<=
    GtGtEq,     // >>=
    GtGtGtEq,   // >>>=
    AmpAmpEq,   // &&=
    PipePipeEq, // ||=
    QuestionQuestionEq, // ??=

    // Comparison
    EqEq,       // ==
    EqEqEq,     // ===
    BangEq,     // !=
    BangEqEq,   // !==
    Lt,         // <
    LtEq,       // <=
    Gt,         // >
    GtEq,       // >=

    // Arithmetic
    Plus,       // +
    Minus,      // -
    Star,       // *
    Slash,      // /
    Percent,    // %
    StarStar,   // **
    PlusPlus,   // ++
    MinusMinus, // --

    // Bitwise
    Amp,        // &
    Pipe,       // |
    Caret,      // ^
    Tilde,      // ~
    LtLt,       // <<
    GtGt,       // >>
    GtGtGt,     // >>>

    // Logical
    AmpAmp,     // &&
    PipePipe,   // ||
    Bang,       // !
    QuestionQuestion, // ??

    // === JSX (when feature enabled) ===
    #[cfg(feature = "jsx")]
    JsxText(String),
    #[cfg(feature = "jsx")]
    JsxTagStart,    // < in JSX context
    #[cfg(feature = "jsx")]
    JsxTagEnd,      // > in JSX context
    #[cfg(feature = "jsx")]
    JsxSelfClose,   // />
    #[cfg(feature = "jsx")]
    JsxCloseTag,    // </

    // === Special ===
    /// End of file
    Eof,
    /// Invalid token (lexer error)
    Invalid,
}

impl TokenKind {
    /// Check if this token can start an expression.
    pub fn can_start_expr(&self) -> bool {
        matches!(
            self,
            TokenKind::Identifier(_)
                | TokenKind::String(_)
                | TokenKind::Number(_)
                | TokenKind::BigInt(_)
                | TokenKind::Regex { .. }
                | TokenKind::TemplateNoSub(_)
                | TokenKind::TemplateHead(_)
                | TokenKind::LParen
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::Function
                | TokenKind::Class
                | TokenKind::New
                | TokenKind::This
                | TokenKind::Super
                | TokenKind::Null
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Async
                | TokenKind::Await
                | TokenKind::Yield
                | TokenKind::Typeof
                | TokenKind::Void
                | TokenKind::Delete
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Bang
                | TokenKind::Tilde
                | TokenKind::PlusPlus
                | TokenKind::MinusMinus
                | TokenKind::Spread
                | TokenKind::Import
        )
    }

    /// Check if this is a keyword.
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenKind::Var
                | TokenKind::Let
                | TokenKind::Const
                | TokenKind::Function
                | TokenKind::Class
                | TokenKind::If
                | TokenKind::Else
                | TokenKind::Switch
                | TokenKind::Case
                | TokenKind::Default
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Do
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Return
                | TokenKind::Try
                | TokenKind::Catch
                | TokenKind::Finally
                | TokenKind::Throw
                | TokenKind::New
                | TokenKind::Delete
                | TokenKind::Typeof
                | TokenKind::Void
                | TokenKind::In
                | TokenKind::Instanceof
                | TokenKind::This
                | TokenKind::Super
                | TokenKind::Null
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Import
                | TokenKind::Export
                | TokenKind::From
                | TokenKind::As
                | TokenKind::Async
                | TokenKind::Await
                | TokenKind::Yield
                | TokenKind::Static
                | TokenKind::Get
                | TokenKind::Set
                | TokenKind::Extends
                | TokenKind::With
                | TokenKind::Debugger
        )
    }

    /// Check if this is an assignment operator.
    pub fn is_assignment(&self) -> bool {
        matches!(
            self,
            TokenKind::Eq
                | TokenKind::PlusEq
                | TokenKind::MinusEq
                | TokenKind::StarEq
                | TokenKind::SlashEq
                | TokenKind::PercentEq
                | TokenKind::StarStarEq
                | TokenKind::AmpEq
                | TokenKind::PipeEq
                | TokenKind::CaretEq
                | TokenKind::LtLtEq
                | TokenKind::GtGtEq
                | TokenKind::GtGtGtEq
                | TokenKind::AmpAmpEq
                | TokenKind::PipePipeEq
                | TokenKind::QuestionQuestionEq
        )
    }

    /// Get the precedence of a binary operator (higher = binds tighter).
    /// Returns None if not a binary operator.
    pub fn binary_precedence(&self) -> Option<u8> {
        match self {
            TokenKind::QuestionQuestion => Some(1),
            TokenKind::PipePipe => Some(2),
            TokenKind::AmpAmp => Some(3),
            TokenKind::Pipe => Some(4),
            TokenKind::Caret => Some(5),
            TokenKind::Amp => Some(6),
            TokenKind::EqEq | TokenKind::EqEqEq | TokenKind::BangEq | TokenKind::BangEqEq => Some(7),
            TokenKind::Lt | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq
            | TokenKind::In | TokenKind::Instanceof => Some(8),
            TokenKind::LtLt | TokenKind::GtGt | TokenKind::GtGtGt => Some(9),
            TokenKind::Plus | TokenKind::Minus => Some(10),
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Some(11),
            TokenKind::StarStar => Some(12), // Right associative
            _ => None,
        }
    }

    /// Check if this binary operator is right associative.
    pub fn is_right_associative(&self) -> bool {
        matches!(self, TokenKind::StarStar)
    }
}

/// Look up a keyword from an identifier string.
pub fn keyword_from_str(s: &str) -> Option<TokenKind> {
    match s {
        "var" => Some(TokenKind::Var),
        "let" => Some(TokenKind::Let),
        "const" => Some(TokenKind::Const),
        "function" => Some(TokenKind::Function),
        "class" => Some(TokenKind::Class),
        "if" => Some(TokenKind::If),
        "else" => Some(TokenKind::Else),
        "switch" => Some(TokenKind::Switch),
        "case" => Some(TokenKind::Case),
        "default" => Some(TokenKind::Default),
        "for" => Some(TokenKind::For),
        "while" => Some(TokenKind::While),
        "do" => Some(TokenKind::Do),
        "break" => Some(TokenKind::Break),
        "continue" => Some(TokenKind::Continue),
        "return" => Some(TokenKind::Return),
        "try" => Some(TokenKind::Try),
        "catch" => Some(TokenKind::Catch),
        "finally" => Some(TokenKind::Finally),
        "throw" => Some(TokenKind::Throw),
        "new" => Some(TokenKind::New),
        "delete" => Some(TokenKind::Delete),
        "typeof" => Some(TokenKind::Typeof),
        "void" => Some(TokenKind::Void),
        "in" => Some(TokenKind::In),
        "instanceof" => Some(TokenKind::Instanceof),
        "this" => Some(TokenKind::This),
        "super" => Some(TokenKind::Super),
        "null" => Some(TokenKind::Null),
        "true" => Some(TokenKind::True),
        "false" => Some(TokenKind::False),
        "import" => Some(TokenKind::Import),
        "export" => Some(TokenKind::Export),
        "from" => Some(TokenKind::From),
        "as" => Some(TokenKind::As),
        "async" => Some(TokenKind::Async),
        "await" => Some(TokenKind::Await),
        "yield" => Some(TokenKind::Yield),
        "static" => Some(TokenKind::Static),
        "get" => Some(TokenKind::Get),
        "set" => Some(TokenKind::Set),
        "extends" => Some(TokenKind::Extends),
        "with" => Some(TokenKind::With),
        "debugger" => Some(TokenKind::Debugger),

        // TypeScript keywords
        #[cfg(feature = "typescript")]
        "type" => Some(TokenKind::Type),
        #[cfg(feature = "typescript")]
        "interface" => Some(TokenKind::Interface),
        #[cfg(feature = "typescript")]
        "enum" => Some(TokenKind::Enum),
        #[cfg(feature = "typescript")]
        "namespace" => Some(TokenKind::Namespace),
        #[cfg(feature = "typescript")]
        "module" => Some(TokenKind::Module),
        #[cfg(feature = "typescript")]
        "declare" => Some(TokenKind::Declare),
        #[cfg(feature = "typescript")]
        "abstract" => Some(TokenKind::Abstract),
        #[cfg(feature = "typescript")]
        "private" => Some(TokenKind::Private),
        #[cfg(feature = "typescript")]
        "protected" => Some(TokenKind::Protected),
        #[cfg(feature = "typescript")]
        "public" => Some(TokenKind::Public),
        #[cfg(feature = "typescript")]
        "readonly" => Some(TokenKind::Readonly),
        #[cfg(feature = "typescript")]
        "override" => Some(TokenKind::Override),
        #[cfg(feature = "typescript")]
        "implements" => Some(TokenKind::Implements),
        #[cfg(feature = "typescript")]
        "is" => Some(TokenKind::Is),
        #[cfg(feature = "typescript")]
        "keyof" => Some(TokenKind::Keyof),
        #[cfg(feature = "typescript")]
        "infer" => Some(TokenKind::Infer),
        #[cfg(feature = "typescript")]
        "never" => Some(TokenKind::Never),
        #[cfg(feature = "typescript")]
        "unknown" => Some(TokenKind::Unknown),
        #[cfg(feature = "typescript")]
        "any" => Some(TokenKind::Any),
        #[cfg(feature = "typescript")]
        "asserts" => Some(TokenKind::Asserts),
        #[cfg(feature = "typescript")]
        "satisfies" => Some(TokenKind::Satisfies),

        _ => None,
    }
}
