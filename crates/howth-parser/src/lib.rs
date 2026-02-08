//! howth-parser: Fast JavaScript/TypeScript/JSX parser
//!
//! Architecture based on esbuild (Go) and Bun (Zig) parsers.
//!
//! # Design Principles
//!
//! 1. **Everything is an Expression, Binding, or Statement**
//!    - Expressions: `foo(1)`, `a + b`, `x.y`
//!    - Bindings: `a`, `[a, b]`, `{x: y}`
//!    - Statements: `let a = 1;`, `if (x) {}`, `return x;`
//!
//! 2. **Lexing on-demand**
//!    - Lexer is called during parsing, not upfront
//!    - Enables context-sensitive tokenization (regex vs division)
//!
//! 3. **Arena-based allocation**
//!    - AST nodes stored in contiguous vectors
//!    - References via indices, not pointers
//!    - Cache-friendly and easy to traverse
//!
//! 4. **Two-pass parsing**
//!    - Pass 1: Parse, declare symbols, build scope tree
//!    - Pass 2: Bind identifiers, lower syntax, optimize
//!
//! # Example
//!
//! ```ignore
//! use howth_parser::{Parser, ParserOptions};
//!
//! let source = "const x = 1 + 2;";
//! let ast = Parser::new(source, ParserOptions::default()).parse()?;
//! ```

#![allow(dead_code)] // During development

mod token;
mod lexer;
mod ast;
mod parser;
mod span;

// Arena allocation (Bun-style speed)
mod arena;
mod ast_arena;
mod parser_arena;

// Feature-gated modules
#[cfg(feature = "typescript")]
mod typescript;

#[cfg(feature = "jsx")]
mod jsx;

mod codegen;

// Re-exports
pub use token::{Token, TokenKind};
pub use lexer::Lexer;
pub use ast::*;
pub use parser::{Parser, ParserOptions, ParseError};
pub use span::Span;
pub use codegen::{Codegen, CodegenOptions};

// Arena-based (fast) API
pub use arena::Arena;
pub use parser_arena::ArenaParser;
pub mod fast {
    //! Fast arena-allocated parsing API.
    //!
    //! Use this for maximum performance. All AST nodes are allocated
    //! in a bump allocator, giving ~2-3x faster parsing.
    //!
    //! ```ignore
    //! use howth_parser::fast::{Arena, ArenaParser, ParserOptions};
    //!
    //! let arena = Arena::new();
    //! let source = "const x = 1 + 2;";
    //! let program = ArenaParser::new(&arena, source, ParserOptions::default()).parse()?;
    //! ```
    pub use crate::arena::Arena;
    pub use crate::ast_arena::*;
    pub use crate::parser_arena::{ArenaParser, ParserOptions, ParseError};
}

/// Parse JavaScript/TypeScript source code into an AST.
pub fn parse(source: &str, options: ParserOptions) -> Result<Ast, ParseError> {
    Parser::new(source, options).parse()
}

/// Parse and generate JavaScript output.
pub fn transform(source: &str, parser_opts: ParserOptions, codegen_opts: CodegenOptions) -> Result<String, ParseError> {
    let ast = parse(source, parser_opts)?;
    Ok(Codegen::new(&ast, codegen_opts).generate())
}
