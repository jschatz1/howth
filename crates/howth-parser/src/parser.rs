//! JavaScript/TypeScript/JSX parser.
//!
//! Uses a recursive descent parser with Pratt parsing for expressions.
//! Based on esbuild and Bun parser architecture.

use crate::ast::*;
use crate::lexer::Lexer;
use crate::span::Span;
use crate::token::{Token, TokenKind};

/// Parser configuration options.
#[derive(Debug, Clone, Default)]
pub struct ParserOptions {
    /// Parse as ECMAScript module (enables import/export).
    pub module: bool,
    /// Enable TypeScript parsing.
    #[cfg(feature = "typescript")]
    pub typescript: bool,
    /// Enable JSX parsing.
    #[cfg(feature = "jsx")]
    pub jsx: bool,
}

/// Parse error.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl ParseError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for ParseError {}

/// The parser.
pub struct Parser<'a> {
    /// The lexer.
    pub(crate) lexer: Lexer<'a>,
    /// Current token.
    pub(crate) current: Token,
    /// Parser options.
    pub(crate) options: ParserOptions,
    /// Source code (for creating AST).
    pub(crate) source: &'a str,
    /// When false, `in` is not parsed as a binary operator (for-in init).
    pub(crate) allow_in: bool,
}

impl<'a> Parser<'a> {
    /// Create a new parser.
    pub fn new(source: &'a str, options: ParserOptions) -> Self {
        let mut lexer = Lexer::new(source);
        let current = lexer.next_token();
        Self {
            lexer,
            current,
            options,
            source,
            allow_in: true,
        }
    }

    /// Parse the entire source into an AST.
    pub fn parse(mut self) -> Result<Ast, ParseError> {
        let stmts = self.parse_program()?;
        Ok(Ast::new(stmts, self.source.to_string()))
    }

    // =========================================================================
    // Token Handling
    // =========================================================================

    /// Get the current token kind.
    pub(crate) fn peek(&self) -> &TokenKind {
        &self.current.kind
    }

    /// Get the current token.
    pub(crate) fn current_token(&self) -> &Token {
        &self.current
    }

    /// Advance to the next token and return the previous.
    pub(crate) fn advance(&mut self) -> Token {
        let prev = std::mem::replace(&mut self.current, self.lexer.next_token());
        prev
    }

    /// Check if the current token matches the given kind.
    pub(crate) fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(kind)
    }

    /// Check if at end of file.
    pub(crate) fn is_eof(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    /// Consume a token if it matches, otherwise return an error.
    pub(crate) fn expect(&mut self, kind: &TokenKind) -> Result<Token, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(ParseError::new(
                format!("Expected {:?}, got {:?}", kind, self.peek()),
                self.current.span,
            ))
        }
    }

    /// Consume a token if it matches, returning true if consumed.
    pub(crate) fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consume a semicolon (with ASI support).
    pub(crate) fn expect_semicolon(&mut self) -> Result<(), ParseError> {
        // Automatic Semicolon Insertion (ASI) rules:
        // 1. Explicit semicolon
        if self.eat(&TokenKind::Semicolon) {
            return Ok(());
        }
        // 2. Before closing brace
        if self.check(&TokenKind::RBrace) {
            return Ok(());
        }
        // 3. At end of file
        if self.is_eof() {
            return Ok(());
        }
        // 4. After newline - the current token was preceded by a line terminator
        if self.current.had_newline_before {
            return Ok(());
        }
        Err(ParseError::new(
            "Expected semicolon",
            self.current.span,
        ))
    }

    // =========================================================================
    // Program Parsing
    // =========================================================================

    /// Parse a program (list of statements).
    fn parse_program(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        while !self.is_eof() {
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    // =========================================================================
    // Statement Parsing
    // =========================================================================

    /// Parse a statement.
    pub(crate) fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;

        // TypeScript: `const enum` needs to be detected before var decl parsing
        #[cfg(feature = "typescript")]
        if matches!(self.peek(), TokenKind::Const) && self.options.typescript {
            let next = self.lexer.peek();
            if matches!(next.kind, TokenKind::Enum) {
                return self.parse_ts_enum();
            }
        }

        // Decorators: @expr (before class or export)
        if self.check(&TokenKind::At) {
            // Consume all decorators
            while self.eat(&TokenKind::At) {
                // Decorator expression: could be @foo, @foo.bar, @foo(), @foo.bar()
                let _ = self.parse_left_hand_side_expr()?;
            }
            // After decorators, expect class, export, or abstract class
            return self.parse_stmt();
        }

        match self.peek() {
            // Declarations
            TokenKind::Var | TokenKind::Let | TokenKind::Const => self.parse_var_decl(),
            TokenKind::Function => self.parse_function_decl(),
            TokenKind::Class => self.parse_class_decl(),

            // Control flow
            TokenKind::If => self.parse_if_stmt(),
            TokenKind::Switch => self.parse_switch_stmt(),
            TokenKind::For => self.parse_for_stmt(),
            TokenKind::While => self.parse_while_stmt(),
            TokenKind::Do => self.parse_do_while_stmt(),
            TokenKind::Break => self.parse_break_stmt(),
            TokenKind::Continue => self.parse_continue_stmt(),
            TokenKind::Return => self.parse_return_stmt(),
            TokenKind::Throw => self.parse_throw_stmt(),
            TokenKind::Try => self.parse_try_stmt(),
            TokenKind::With => self.parse_with_stmt(),
            TokenKind::Debugger => self.parse_debugger_stmt(),

            // Block
            TokenKind::LBrace => self.parse_block_stmt(),

            // Empty statement
            TokenKind::Semicolon => {
                self.advance();
                Ok(Stmt::new(StmtKind::Empty, Span::new(start, self.current.span.start)))
            }

            // Module declarations
            TokenKind::Import => self.parse_import_decl(),
            TokenKind::Export => self.parse_export_decl(),

            // TypeScript declarations
            #[cfg(feature = "typescript")]
            TokenKind::Type if self.options.typescript => self.parse_ts_type_alias(),
            #[cfg(feature = "typescript")]
            TokenKind::Interface => self.parse_ts_interface(),
            #[cfg(feature = "typescript")]
            TokenKind::Enum => self.parse_ts_enum(),
            #[cfg(feature = "typescript")]
            TokenKind::Namespace | TokenKind::Module => {
                if self.options.typescript {
                    let next = self.lexer.peek();
                    if matches!(next.kind, TokenKind::Dot | TokenKind::Colon | TokenKind::Eq) {
                        // module.exports, module: label, module = expr — expression, not namespace
                        self.parse_expr_stmt()
                    } else {
                        self.parse_ts_namespace()
                    }
                } else {
                    self.parse_expr_stmt()
                }
            }
            #[cfg(feature = "typescript")]
            TokenKind::Declare => self.parse_ts_declare(),
            #[cfg(feature = "typescript")]
            TokenKind::Abstract => {
                // abstract class
                self.advance();
                self.parse_class_decl()
            }

            // Async function (lookahead required)
            TokenKind::Async => {
                // TODO: Check if followed by function keyword
                self.parse_expr_stmt()
            }

            // Labeled statement or expression statement
            TokenKind::Identifier(ref name) => {
                let name = name.clone();
                // Check for labeled statement: `label: stmt`
                if matches!(self.lexer.peek().kind, TokenKind::Colon) {
                    let label = name.clone();
                    self.advance(); // consume label
                    self.advance(); // consume ':'
                    let body = self.parse_stmt()?;
                    let end = self.current.span.start;
                    return Ok(Stmt::new(StmtKind::Labeled { label, body: Box::new(body) }, Span::new(start, end)));
                }
                match name.as_str() {
                    // Bare `global { ... }` augmentation in TS
                    #[cfg(feature = "typescript")]
                    "global" if self.options.typescript => {
                        if matches!(self.lexer.peek().kind, TokenKind::LBrace) {
                            self.advance(); // consume `global`
                            self.advance(); // consume `{`
                            while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                                self.parse_stmt()?;
                            }
                            self.expect(&TokenKind::RBrace)?;
                            let end = self.current.span.start;
                            Ok(Stmt::new(StmtKind::Empty, Span::new(start, end)))
                        } else {
                            self.parse_expr_stmt()
                        }
                    }
                    // `using x = ...` declarations (TC39 explicit resource management)
                    "using" => {
                        // Check if followed by identifier → using declaration
                        if matches!(self.lexer.peek().kind, TokenKind::Identifier(_)) {
                            self.advance(); // consume `using`
                            let mut decls = Vec::new();
                            loop {
                                decls.push(self.parse_var_declarator()?);
                                if !self.eat(&TokenKind::Comma) { break; }
                            }
                            self.expect_semicolon()?;
                            let end = self.current.span.start;
                            return Ok(Stmt::new(StmtKind::Var { kind: VarKind::Const, decls }, Span::new(start, end)));
                        }
                        self.parse_expr_stmt()
                    }
                    _ => self.parse_expr_stmt(),
                }
            }

            // Expression statement
            _ => self.parse_expr_stmt(),
        }
    }

    /// Parse a block statement.
    fn parse_block_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::LBrace)?;

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            stmts.push(self.parse_stmt()?);
        }

        let end = self.current.span.end;
        self.expect(&TokenKind::RBrace)?;

        Ok(Stmt::new(StmtKind::Block(stmts), Span::new(start, end)))
    }

    /// Parse variable declaration.
    pub(crate) fn parse_var_decl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;

        let kind = match self.peek() {
            TokenKind::Var => VarKind::Var,
            TokenKind::Let => VarKind::Let,
            TokenKind::Const => VarKind::Const,
            _ => unreachable!(),
        };
        self.advance();

        let mut decls = Vec::new();
        loop {
            decls.push(self.parse_var_declarator()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        self.expect_semicolon()?;
        let end = self.current.span.start;

        Ok(Stmt::new(StmtKind::Var { kind, decls }, Span::new(start, end)))
    }

    /// Parse a variable declarator.
    fn parse_var_declarator(&mut self) -> Result<VarDeclarator, ParseError> {
        let start = self.current.span.start;
        let binding = self.parse_binding()?;

        // TypeScript: definite assignment `!` and type annotation after binding pattern
        #[cfg(feature = "typescript")]
        if self.options.typescript {
            self.eat(&TokenKind::Bang); // definite assignment assertion: let x!: Type
            if self.eat(&TokenKind::Colon) {
                let _ = self.parse_ts_type()?;
            }
        }

        let init = if self.eat(&TokenKind::Eq) {
            Some(self.parse_assign_expr()?)
        } else {
            None
        };

        let end = self.current.span.start;
        Ok(VarDeclarator {
            binding,
            init,
            span: Span::new(start, end),
        })
    }

    /// Parse a binding pattern.
    fn parse_binding(&mut self) -> Result<Binding, ParseError> {
        let start = self.current.span.start;

        match self.peek() {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                let end = self.current.span.start;

                #[cfg(feature = "typescript")]
                let type_ann = if self.eat(&TokenKind::Colon) {
                    Some(Box::new(self.parse_ts_type()?))
                } else {
                    None
                };

                Ok(Binding::new(
                    BindingKind::Ident {
                        name,
                        #[cfg(feature = "typescript")]
                        type_ann,
                    },
                    Span::new(start, end),
                ))
            }
            TokenKind::LBracket => self.parse_array_binding(),
            TokenKind::LBrace => self.parse_object_binding(),
            // JS contextual keywords that can be used as binding names
            TokenKind::Get | TokenKind::Set | TokenKind::From
            | TokenKind::As | TokenKind::Static | TokenKind::Async
            | TokenKind::Let | TokenKind::Yield => {
                let name = keyword_to_str(self.peek()).to_string();
                self.advance();
                let end = self.current.span.start;
                #[cfg(feature = "typescript")]
                let type_ann = if self.eat(&TokenKind::Colon) {
                    Some(Box::new(self.parse_ts_type()?))
                } else {
                    None
                };
                Ok(Binding::new(
                    BindingKind::Ident {
                        name,
                        #[cfg(feature = "typescript")]
                        type_ann,
                    },
                    Span::new(start, end),
                ))
            }
            // TypeScript: `this` as parameter name (e.g. `function foo(this: Window)`)
            #[cfg(feature = "typescript")]
            TokenKind::This if self.options.typescript => {
                self.advance();
                let end = self.current.span.start;
                let type_ann = if self.eat(&TokenKind::Colon) {
                    Some(Box::new(self.parse_ts_type()?))
                } else {
                    None
                };
                Ok(Binding::new(
                    BindingKind::Ident {
                        name: "this".to_string(),
                        type_ann,
                    },
                    Span::new(start, end),
                ))
            }
            // TypeScript contextual keywords as binding names
            #[cfg(feature = "typescript")]
            _ if self.options.typescript && crate::typescript::is_ts_contextual_keyword(self.peek()) => {
                let name = self.expect_ts_identifier()?;
                let end = self.current.span.start;
                let type_ann = if self.eat(&TokenKind::Colon) {
                    Some(Box::new(self.parse_ts_type()?))
                } else {
                    None
                };
                Ok(Binding::new(
                    BindingKind::Ident {
                        name,
                        type_ann,
                    },
                    Span::new(start, end),
                ))
            }
            _ => Err(ParseError::new(
                format!("Expected identifier, '[', or '{{', got {:?}", self.peek()),
                self.current.span,
            )),
        }
    }

    /// Parse array binding pattern: `[a, b, ...rest]`
    fn parse_array_binding(&mut self) -> Result<Binding, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::LBracket)?;

        let mut elements = Vec::new();
        while !self.check(&TokenKind::RBracket) && !self.is_eof() {
            if self.check(&TokenKind::Comma) {
                // Elision
                elements.push(None);
            } else {
                let rest = self.eat(&TokenKind::Spread);
                let binding = self.parse_binding()?;
                let default = if self.eat(&TokenKind::Eq) {
                    Some(self.parse_assign_expr()?)
                } else {
                    None
                };
                elements.push(Some(ArrayPatternElement { binding, default, rest }));
            }

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        let end = self.current.span.end;
        self.expect(&TokenKind::RBracket)?;

        Ok(Binding::new(
            BindingKind::Array {
                elements,
                #[cfg(feature = "typescript")]
                type_ann: None,
            },
            Span::new(start, end),
        ))
    }

    /// Parse object binding pattern: `{a, b: c, ...rest}`
    fn parse_object_binding(&mut self) -> Result<Binding, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::LBrace)?;

        let mut properties = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            let rest = self.eat(&TokenKind::Spread);

            if rest {
                // Rest element: `...rest`
                let binding = self.parse_binding()?;
                properties.push(ObjectPatternProperty {
                    key: PropertyKey::Ident(String::new()),
                    value: binding,
                    default: None,
                    shorthand: false,
                    rest: true,
                });
            } else {
                // Property: `key` or `key: value` or `key = default`
                let key = self.parse_property_key()?;

                if self.eat(&TokenKind::Colon) {
                    // `key: value`
                    let value = self.parse_binding()?;
                    let default = if self.eat(&TokenKind::Eq) {
                        Some(self.parse_assign_expr()?)
                    } else {
                        None
                    };
                    properties.push(ObjectPatternProperty {
                        key,
                        value,
                        default,
                        shorthand: false,
                        rest: false,
                    });
                } else {
                    // Shorthand: `key` or `key = default`
                    let name = match &key {
                        PropertyKey::Ident(n) => n.clone(),
                        _ => return Err(ParseError::new(
                            "Expected identifier in shorthand property",
                            self.current.span,
                        )),
                    };
                    let default = if self.eat(&TokenKind::Eq) {
                        Some(self.parse_assign_expr()?)
                    } else {
                        None
                    };
                    properties.push(ObjectPatternProperty {
                        key,
                        value: Binding::new(
                            BindingKind::Ident {
                                name,
                                #[cfg(feature = "typescript")]
                                type_ann: None,
                            },
                            self.current.span,
                        ),
                        default,
                        shorthand: true,
                        rest: false,
                    });
                }
            }

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        let end = self.current.span.end;
        self.expect(&TokenKind::RBrace)?;

        Ok(Binding::new(
            BindingKind::Object {
                properties,
                #[cfg(feature = "typescript")]
                type_ann: None,
            },
            Span::new(start, end),
        ))
    }

    /// Parse a property key.
    pub(crate) fn parse_property_key(&mut self) -> Result<PropertyKey, ParseError> {
        match self.peek() {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                Ok(PropertyKey::Ident(name))
            }
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(PropertyKey::String(s))
            }
            TokenKind::Number(n) => {
                let n = *n;
                self.advance();
                Ok(PropertyKey::Number(n))
            }
            TokenKind::LBracket => {
                self.advance();
                let expr = self.parse_assign_expr()?;
                self.expect(&TokenKind::RBracket)?;
                Ok(PropertyKey::Computed(Box::new(expr)))
            }
            // Private field/method: #name
            TokenKind::Hash => {
                self.advance();
                let name = if let TokenKind::Identifier(n) = self.peek() {
                    let n = n.clone();
                    self.advance();
                    n
                } else {
                    return Err(ParseError::new("Expected identifier after '#'", self.current.span));
                };
                Ok(PropertyKey::Ident(format!("#{}", name)))
            }
            // Keywords that can be used as property names
            _ if self.peek().is_keyword() => {
                let name = keyword_to_str(self.peek()).to_string();
                self.advance();
                Ok(PropertyKey::Ident(name))
            }
            // TypeScript contextual keywords as property keys
            #[cfg(feature = "typescript")]
            _ if crate::typescript::is_ts_contextual_keyword(self.peek()) => {
                let name = self.expect_ts_identifier()?;
                Ok(PropertyKey::Ident(name))
            }
            _ => Err(ParseError::new(
                format!("Expected property key, got {:?}", self.peek()),
                self.current.span,
            )),
        }
    }

    /// Parse function declaration.
    pub(crate) fn parse_function_decl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        let func = self.parse_function(false)?;
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::Function(Box::new(func)), Span::new(start, end)))
    }

    /// Parse a function.
    fn parse_function(&mut self, is_async: bool) -> Result<Function, ParseError> {
        let start = self.current.span.start;

        self.expect(&TokenKind::Function)?;
        let is_generator = self.eat(&TokenKind::Star);

        // Function name (optional for expressions)
        let name = match self.peek() {
            TokenKind::Identifier(n) => {
                let n = n.clone();
                self.advance();
                Some(n)
            }
            #[cfg(feature = "typescript")]
            _ if self.options.typescript && crate::typescript::is_ts_contextual_keyword(self.peek()) => {
                Some(self.expect_ts_identifier().unwrap_or_default())
            }
            // JS keywords used as function names (e.g., `declare function get<T>()`)
            _ if self.peek().is_keyword() => {
                let n = keyword_to_str(self.peek()).to_string();
                self.advance();
                Some(n)
            }
            _ => None,
        };

        // TypeScript type parameters
        #[cfg(feature = "typescript")]
        let type_params = if self.options.typescript && self.check(&TokenKind::Lt) {
            Some(self.parse_ts_type_params_impl()?)
        } else {
            None
        };

        // Parameters
        let params = self.parse_params()?;

        // TypeScript return type
        #[cfg(feature = "typescript")]
        let return_type = if self.options.typescript && self.eat(&TokenKind::Colon) {
            Some(Box::new(self.parse_ts_type()?))
        } else {
            None
        };

        // Body (may be absent in TypeScript declare context)
        let (body, end) = if self.check(&TokenKind::LBrace) {
            self.expect(&TokenKind::LBrace)?;
            let mut body = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                body.push(self.parse_stmt()?);
            }
            let end = self.current.span.end;
            self.expect(&TokenKind::RBrace)?;
            (body, end)
        } else {
            // Ambient function (declare context) — no body
            self.expect_semicolon()?;
            (Vec::new(), self.current.span.start)
        };

        Ok(Function {
            name,
            params,
            body,
            is_async,
            is_generator,
            span: Span::new(start, end),
            #[cfg(feature = "typescript")]
            type_params,
            #[cfg(feature = "typescript")]
            return_type,
        })
    }

    /// Parse function parameters.
    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect(&TokenKind::LParen)?;

        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.is_eof() {
            let start = self.current.span.start;

            // Parameter decorators: @decorator
            while self.eat(&TokenKind::At) {
                let _ = self.parse_left_hand_side_expr()?;
            }

            // TypeScript: consume accessibility modifier on constructor params
            #[cfg(feature = "typescript")]
            if self.options.typescript {
                self.try_parse_accessibility();
                // Also consume readonly
                self.eat(&TokenKind::Readonly);
            }

            let rest = self.eat(&TokenKind::Spread);
            let binding = self.parse_binding()?;

            // TypeScript: optional parameter marker `?` and type annotation after it
            #[cfg(feature = "typescript")]
            if self.options.typescript {
                self.eat(&TokenKind::Question);
                // After `?`, there may be a `: type` annotation (e.g. `x?: number`)
                if self.eat(&TokenKind::Colon) {
                    let _ = self.parse_ts_type()?;
                }
            }

            let default = if self.eat(&TokenKind::Eq) {
                Some(self.parse_assign_expr()?)
            } else {
                None
            };
            let end = self.current.span.start;

            params.push(Param {
                binding,
                default,
                rest,
                span: Span::new(start, end),
            });

            if rest || !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        self.expect(&TokenKind::RParen)?;
        Ok(params)
    }

    /// Parse function parameters WITHOUT the surrounding parentheses.
    /// Used by arrow function parsing when `(` has already been consumed.
    pub(crate) fn parse_params_inner(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.is_eof() {
            let start = self.current.span.start;
            // Parameter decorators: @decorator
            while self.eat(&TokenKind::At) {
                let _ = self.parse_left_hand_side_expr()?;
            }
            #[cfg(feature = "typescript")]
            if self.options.typescript {
                self.try_parse_accessibility();
                self.eat(&TokenKind::Readonly);
            }
            let rest = self.eat(&TokenKind::Spread);
            let binding = self.parse_binding()?;
            #[cfg(feature = "typescript")]
            if self.options.typescript {
                self.eat(&TokenKind::Question);
                if self.eat(&TokenKind::Colon) {
                    let _ = self.parse_ts_type()?;
                }
            }
            let default = if self.eat(&TokenKind::Eq) {
                Some(self.parse_assign_expr()?)
            } else {
                None
            };
            let end = self.current.span.start;
            params.push(Param { binding, default, rest, span: Span::new(start, end) });
            if rest || !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        Ok(params)
    }

    /// Parse class declaration.
    pub(crate) fn parse_class_decl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        let class = self.parse_class()?;
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::Class(Box::new(class)), Span::new(start, end)))
    }

    /// Parse a class.
    fn parse_class(&mut self) -> Result<Class, ParseError> {
        let start = self.current.span.start;

        self.expect(&TokenKind::Class)?;

        // Class name (optional for expressions)
        let name = if let TokenKind::Identifier(n) = self.peek() {
            let n = n.clone();
            self.advance();
            Some(n)
        } else {
            None
        };

        // TypeScript type parameters
        #[cfg(feature = "typescript")]
        let type_params = if self.options.typescript && self.check(&TokenKind::Lt) {
            Some(self.parse_ts_type_params_impl()?)
        } else {
            None
        };

        // Extends clause
        let super_class = if self.eat(&TokenKind::Extends) {
            Some(Box::new(self.parse_left_hand_side_expr()?))
        } else {
            None
        };

        // TypeScript: consume type args on super class (e.g., `extends Base<T>`)
        #[cfg(feature = "typescript")]
        if self.options.typescript && self.check(&TokenKind::Lt) {
            let _ = self.parse_ts_type_args_impl()?;
        }

        // TypeScript implements clause
        #[cfg(feature = "typescript")]
        let mut implements = Vec::new();
        #[cfg(feature = "typescript")]
        if self.options.typescript && self.eat(&TokenKind::Implements) {
            loop {
                implements.push(self.parse_ts_type()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }

        // Body
        self.expect(&TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            if self.check(&TokenKind::Semicolon) {
                self.advance();
                continue;
            }
            body.push(self.parse_class_member()?);
        }
        let end = self.current.span.end;
        self.expect(&TokenKind::RBrace)?;

        Ok(Class {
            name,
            super_class,
            body,
            span: Span::new(start, end),
            #[cfg(feature = "typescript")]
            type_params,
            #[cfg(feature = "typescript")]
            implements,
        })
    }

    /// Parse a class member.
    fn parse_class_member(&mut self) -> Result<ClassMember, ParseError> {
        let start = self.current.span.start;

        // Decorators on class members: @decorator
        while self.eat(&TokenKind::At) {
            let _ = self.parse_left_hand_side_expr()?;
        }

        // TypeScript modifiers: accessibility, abstract, readonly, override
        #[cfg(feature = "typescript")]
        let accessibility = if self.options.typescript {
            self.try_parse_accessibility()
        } else {
            None
        };
        #[cfg(feature = "typescript")]
        let is_abstract = self.options.typescript && self.eat(&TokenKind::Abstract);
        #[cfg(feature = "typescript")]
        let is_readonly = self.options.typescript && self.eat(&TokenKind::Readonly);
        #[cfg(feature = "typescript")]
        let is_override = self.options.typescript && self.eat(&TokenKind::Override);
        #[cfg(feature = "typescript")]
        let _is_declare = self.options.typescript && self.eat(&TokenKind::Declare);

        // Check for static
        let is_static = self.eat(&TokenKind::Static);

        // Consume modifiers that may appear after `static`
        #[cfg(feature = "typescript")]
        if self.options.typescript {
            // These can appear before or after `static`
            if !is_abstract { self.eat(&TokenKind::Abstract); }
            if !is_readonly { self.eat(&TokenKind::Readonly); }
            if !is_override { self.eat(&TokenKind::Override); }
        }

        // Static block
        if is_static && self.check(&TokenKind::LBrace) {
            self.expect(&TokenKind::LBrace)?;
            let mut stmts = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                stmts.push(self.parse_stmt()?);
            }
            let end = self.current.span.end;
            self.expect(&TokenKind::RBrace)?;
            return Ok(ClassMember {
                kind: ClassMemberKind::StaticBlock(stmts),
                span: Span::new(start, end),
            });
        }

        // ES decorator `accessor` keyword: `accessor name: type = value`
        // Consume it as a modifier (treated like a property after)
        if matches!(self.peek(), TokenKind::Identifier(ref n) if n == "accessor")
            && !matches!(self.lexer.peek().kind, TokenKind::LParen | TokenKind::Colon | TokenKind::Eq | TokenKind::Semicolon)
        {
            self.advance(); // consume `accessor`
        }

        // Method kind: get/set are getters/setters UNLESS followed by `(`, `:`, `=`, `;`, `<`
        // `<` means it's a generic method named "get"/"set", not a getter/setter
        let mut method_kind = MethodKind::Method;
        if self.check(&TokenKind::Get) && !matches!(self.lexer.peek().kind, TokenKind::LParen | TokenKind::Colon | TokenKind::Eq | TokenKind::Semicolon | TokenKind::Lt) {
            self.advance();
            method_kind = MethodKind::Get;
        } else if self.check(&TokenKind::Set) && !matches!(self.lexer.peek().kind, TokenKind::LParen | TokenKind::Colon | TokenKind::Eq | TokenKind::Semicolon | TokenKind::Lt) {
            self.advance();
            method_kind = MethodKind::Set;
        }

        // Check for async method
        let is_async_method = self.check(&TokenKind::Async) && !matches!(self.lexer.peek().kind, TokenKind::LParen | TokenKind::Colon | TokenKind::Eq | TokenKind::Semicolon);
        if is_async_method {
            self.advance();
        }

        // Check for generator method
        let is_generator = self.eat(&TokenKind::Star);

        // TypeScript: index signature [key: type]: type;
        #[cfg(feature = "typescript")]
        if self.options.typescript && self.check(&TokenKind::LBracket) {
            // Lookahead: [ identifier : → index signature
            let is_index_sig = {
                let next = self.lexer.peek();
                match &next.kind {
                    TokenKind::Identifier(_) => {
                        let saved = self.lexer.clone();
                        let _ = self.lexer.next_token(); // skip identifier
                        let third = self.lexer.peek();
                        let result = matches!(third.kind, TokenKind::Colon);
                        self.lexer = saved;
                        result
                    }
                    _ => false,
                }
            };
            if is_index_sig {
                // Skip the entire index signature: [key: type]: type;
                self.advance(); // eat [
                let _ = self.expect_identifier()?; // key name
                self.expect(&TokenKind::Colon)?;
                let _ = self.parse_ts_type()?; // param type
                self.expect(&TokenKind::RBracket)?;
                if self.eat(&TokenKind::Colon) {
                    let _ = self.parse_ts_type()?; // value type
                }
                self.expect_semicolon()?;
                let end = self.current.span.start;
                return Ok(ClassMember {
                    kind: ClassMemberKind::Property {
                        key: PropertyKey::Ident("__index".to_string()),
                        value: None,
                        computed: false,
                        is_static,
                        #[cfg(feature = "typescript")]
                        accessibility,
                        #[cfg(feature = "typescript")]
                        is_abstract,
                        #[cfg(feature = "typescript")]
                        is_readonly,
                        #[cfg(feature = "typescript")]
                        is_override,
                        #[cfg(feature = "typescript")]
                        definite: false,
                        #[cfg(feature = "typescript")]
                        type_ann: None,
                    },
                    span: Span::new(start, end),
                });
            }
        }

        // Property key
        let computed = self.check(&TokenKind::LBracket);
        let key = self.parse_property_key()?;

        // Check for constructor
        if matches!(&key, PropertyKey::Ident(n) if n == "constructor") && !is_static {
            method_kind = MethodKind::Constructor;
        }

        // TypeScript: optional `?` or definite `!` marker
        #[cfg(feature = "typescript")]
        let definite = if self.options.typescript {
            self.eat(&TokenKind::Question);
            self.eat(&TokenKind::Bang)
        } else {
            false
        };

        // Method or property?
        if self.check(&TokenKind::LParen) || self.check(&TokenKind::Lt) {
            // TypeScript type parameters on method
            #[cfg(feature = "typescript")]
            let type_params = if self.options.typescript && self.check(&TokenKind::Lt) {
                Some(self.parse_ts_type_params_impl()?)
            } else {
                None
            };

            // Method
            let params = self.parse_params()?;

            // TypeScript return type
            #[cfg(feature = "typescript")]
            let return_type = if self.options.typescript && self.eat(&TokenKind::Colon) {
                Some(Box::new(self.parse_ts_type()?))
            } else {
                None
            };

            // Method body — optional in TypeScript (abstract/declare context)
            let (body, end) = if self.check(&TokenKind::LBrace) {
                self.expect(&TokenKind::LBrace)?;
                let mut body = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                    body.push(self.parse_stmt()?);
                }
                let end = self.current.span.end;
                self.expect(&TokenKind::RBrace)?;
                (body, end)
            } else {
                // Abstract or ambient method — no body, terminated by `;` or newline
                self.expect_semicolon()?;
                (Vec::new(), self.current.span.start)
            };

            let func = Function {
                name: None,
                params,
                body,
                is_async: is_async_method,
                is_generator,
                span: Span::new(start, end),
                #[cfg(feature = "typescript")]
                type_params,
                #[cfg(feature = "typescript")]
                return_type,
            };

            Ok(ClassMember {
                kind: ClassMemberKind::Method {
                    key,
                    value: func,
                    kind: method_kind,
                    computed,
                    is_static,
                    #[cfg(feature = "typescript")]
                    accessibility,
                    #[cfg(feature = "typescript")]
                    is_abstract,
                    #[cfg(feature = "typescript")]
                    is_override,
                },
                span: Span::new(start, end),
            })
        } else {
            // TypeScript: type annotation on property
            #[cfg(feature = "typescript")]
            let type_ann = if self.options.typescript && self.eat(&TokenKind::Colon) {
                Some(Box::new(self.parse_ts_type()?))
            } else {
                None
            };

            // Property
            let value = if self.eat(&TokenKind::Eq) {
                Some(self.parse_assign_expr()?)
            } else {
                None
            };
            self.expect_semicolon()?;
            let end = self.current.span.start;

            Ok(ClassMember {
                kind: ClassMemberKind::Property {
                    key,
                    value,
                    computed,
                    is_static,
                    #[cfg(feature = "typescript")]
                    type_ann,
                    #[cfg(feature = "typescript")]
                    accessibility,
                    #[cfg(feature = "typescript")]
                    is_readonly,
                    #[cfg(feature = "typescript")]
                    is_abstract,
                    #[cfg(feature = "typescript")]
                    is_override,
                    #[cfg(feature = "typescript")]
                    definite,
                },
                span: Span::new(start, end),
            })
        }
    }

    // =========================================================================
    // Control Flow Statements
    // =========================================================================

    fn parse_if_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::If)?;
        self.expect(&TokenKind::LParen)?;
        let test = self.parse_expr()?;
        self.expect(&TokenKind::RParen)?;
        let consequent = Box::new(self.parse_stmt()?);
        let alternate = if self.eat(&TokenKind::Else) {
            Some(Box::new(self.parse_stmt()?))
        } else {
            None
        };
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::If { test, consequent, alternate }, Span::new(start, end)))
    }

    fn parse_switch_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Switch)?;
        self.expect(&TokenKind::LParen)?;
        let discriminant = self.parse_expr()?;
        self.expect(&TokenKind::RParen)?;
        self.expect(&TokenKind::LBrace)?;

        let mut cases = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            let case_start = self.current.span.start;
            let test = if self.eat(&TokenKind::Case) {
                Some(self.parse_expr()?)
            } else if self.eat(&TokenKind::Default) {
                None
            } else {
                return Err(ParseError::new(
                    "Expected 'case' or 'default'",
                    self.current.span,
                ));
            };
            self.expect(&TokenKind::Colon)?;

            let mut consequent = Vec::new();
            while !self.check(&TokenKind::Case)
                && !self.check(&TokenKind::Default)
                && !self.check(&TokenKind::RBrace)
                && !self.is_eof()
            {
                consequent.push(self.parse_stmt()?);
            }
            let case_end = self.current.span.start;
            cases.push(SwitchCase { test, consequent, span: Span::new(case_start, case_end) });
        }

        let end = self.current.span.end;
        self.expect(&TokenKind::RBrace)?;
        Ok(Stmt::new(StmtKind::Switch { discriminant, cases }, Span::new(start, end)))
    }

    fn parse_for_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::For)?;

        let is_await = self.eat(&TokenKind::Await);

        self.expect(&TokenKind::LParen)?;

        // Parse init (variable declaration or expression)
        let init = if self.check(&TokenKind::Semicolon) {
            None
        } else if matches!(self.peek(), TokenKind::Var | TokenKind::Let | TokenKind::Const) {
            let kind = match self.peek() {
                TokenKind::Var => VarKind::Var,
                TokenKind::Let => VarKind::Let,
                TokenKind::Const => VarKind::Const,
                _ => unreachable!(),
            };
            self.advance();
            let mut decls = Vec::new();
            decls.push(self.parse_var_declarator()?);
            while self.eat(&TokenKind::Comma) {
                decls.push(self.parse_var_declarator()?);
            }
            Some(ForInit::Var { kind, decls })
        } else {
            self.allow_in = false;
            let expr = self.parse_expr()?;
            self.allow_in = true;
            Some(ForInit::Expr(expr))
        };

        // Check for for-in or for-of
        if self.eat(&TokenKind::In) {
            let left = init.ok_or_else(|| ParseError::new(
                "Expected variable or expression before 'in'",
                self.current.span,
            ))?;
            let right = self.parse_expr()?;
            self.expect(&TokenKind::RParen)?;
            let body = Box::new(self.parse_stmt()?);
            let end = self.current.span.start;
            return Ok(Stmt::new(StmtKind::ForIn { left, right, body }, Span::new(start, end)));
        }

        // Check for 'of' (identifier, not keyword)
        if let TokenKind::Identifier(id) = self.peek() {
            if id == "of" {
                self.advance();
                let left = init.ok_or_else(|| ParseError::new(
                    "Expected variable or expression before 'of'",
                    self.current.span,
                ))?;
                let right = self.parse_assign_expr()?;
                self.expect(&TokenKind::RParen)?;
                let body = Box::new(self.parse_stmt()?);
                let end = self.current.span.start;
                return Ok(Stmt::new(StmtKind::ForOf { left, right, body, is_await }, Span::new(start, end)));
            }
        }

        // Regular for loop
        self.expect(&TokenKind::Semicolon)?;
        let test = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(&TokenKind::Semicolon)?;
        let update = if self.check(&TokenKind::RParen) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(&TokenKind::RParen)?;
        let body = Box::new(self.parse_stmt()?);
        let end = self.current.span.start;

        Ok(Stmt::new(StmtKind::For { init, test, update, body }, Span::new(start, end)))
    }

    fn parse_while_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::While)?;
        self.expect(&TokenKind::LParen)?;
        let test = self.parse_expr()?;
        self.expect(&TokenKind::RParen)?;
        let body = Box::new(self.parse_stmt()?);
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::While { test, body }, Span::new(start, end)))
    }

    fn parse_do_while_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Do)?;
        let body = Box::new(self.parse_stmt()?);
        self.expect(&TokenKind::While)?;
        self.expect(&TokenKind::LParen)?;
        let test = self.parse_expr()?;
        self.expect(&TokenKind::RParen)?;
        self.expect_semicolon()?;
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::DoWhile { body, test }, Span::new(start, end)))
    }

    fn parse_break_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Break)?;
        let label = if let TokenKind::Identifier(name) = self.peek() {
            let name = name.clone();
            self.advance();
            Some(name)
        } else {
            None
        };
        self.expect_semicolon()?;
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::Break { label }, Span::new(start, end)))
    }

    fn parse_continue_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Continue)?;
        let label = if let TokenKind::Identifier(name) = self.peek() {
            let name = name.clone();
            self.advance();
            Some(name)
        } else {
            None
        };
        self.expect_semicolon()?;
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::Continue { label }, Span::new(start, end)))
    }

    fn parse_return_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Return)?;
        let arg = if self.check(&TokenKind::Semicolon) || self.check(&TokenKind::RBrace) || self.is_eof() {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_semicolon()?;
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::Return { arg }, Span::new(start, end)))
    }

    fn parse_throw_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Throw)?;
        let arg = self.parse_expr()?;
        self.expect_semicolon()?;
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::Throw { arg }, Span::new(start, end)))
    }

    fn parse_try_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Try)?;

        // Try block
        self.expect(&TokenKind::LBrace)?;
        let mut block = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            block.push(self.parse_stmt()?);
        }
        self.expect(&TokenKind::RBrace)?;

        // Catch clause
        let handler = if self.eat(&TokenKind::Catch) {
            let catch_start = self.current.span.start;
            let param = if self.eat(&TokenKind::LParen) {
                let binding = self.parse_binding()?;
                self.expect(&TokenKind::RParen)?;
                Some(binding)
            } else {
                None
            };
            self.expect(&TokenKind::LBrace)?;
            let mut body = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                body.push(self.parse_stmt()?);
            }
            let catch_end = self.current.span.end;
            self.expect(&TokenKind::RBrace)?;
            Some(CatchClause { param, body, span: Span::new(catch_start, catch_end) })
        } else {
            None
        };

        // Finally clause
        let finalizer = if self.eat(&TokenKind::Finally) {
            self.expect(&TokenKind::LBrace)?;
            let mut body = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                body.push(self.parse_stmt()?);
            }
            self.expect(&TokenKind::RBrace)?;
            Some(body)
        } else {
            None
        };

        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::Try { block, handler, finalizer }, Span::new(start, end)))
    }

    fn parse_with_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::With)?;
        self.expect(&TokenKind::LParen)?;
        let object = self.parse_expr()?;
        self.expect(&TokenKind::RParen)?;
        let body = Box::new(self.parse_stmt()?);
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::With { object, body }, Span::new(start, end)))
    }

    fn parse_debugger_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Debugger)?;
        self.expect_semicolon()?;
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::Debugger, Span::new(start, end)))
    }

    // =========================================================================
    // Module Declarations
    // =========================================================================

    fn parse_import_decl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Import)?;

        // Check for import("module") expression
        if self.check(&TokenKind::LParen) {
            // This is actually an expression statement
            let expr = self.parse_call_expr(Expr::new(ExprKind::Ident("import".to_string()), Span::new(start, start + 6)))?;
            self.expect_semicolon()?;
            let end = self.current.span.start;
            return Ok(Stmt::new(StmtKind::Expr(expr), Span::new(start, end)));
        }

        #[cfg(feature = "typescript")]
        let is_type_only = if self.options.typescript && self.check(&TokenKind::Type) {
            // `import type ...` — but only if followed by identifier, `{`, or `*`
            // (not `import type from "mod"` which is a default import of `type`)
            let next = self.lexer.peek();
            match &next.kind {
                TokenKind::Identifier(_) | TokenKind::LBrace | TokenKind::Star => {
                    self.advance();
                    true
                }
                _ => false,
            }
        } else {
            false
        };

        let mut specifiers = Vec::new();

        // Check for string literal (side-effect import)
        if let TokenKind::String(source) = self.peek() {
            let source = source.clone();
            self.advance();
            self.expect_semicolon()?;
            let end = self.current.span.start;
            return Ok(Stmt::new(
                StmtKind::Import(Box::new(ImportDecl {
                    specifiers,
                    source,
                    span: Span::new(start, end),
                    #[cfg(feature = "typescript")]
                    is_type_only,
                })),
                Span::new(start, end),
            ));
        }

        // Default import
        if let TokenKind::Identifier(name) = self.peek() {
            let name = name.clone();
            let spec_start = self.current.span.start;
            self.advance();

            // TypeScript: `import X = require("mod")` or `import X = A.B.C`
            #[cfg(feature = "typescript")]
            if self.options.typescript && self.eat(&TokenKind::Eq) {
                if matches!(self.peek(), TokenKind::Identifier(ref n) if n == "require") {
                    // import X = require("mod") → treat as require call (stripped to: const X = require("mod"))
                    self.advance(); // eat `require`
                    self.expect(&TokenKind::LParen)?;
                    let source = self.expect_string()?;
                    self.expect(&TokenKind::RParen)?;
                    self.expect_semicolon()?;
                    let end = self.current.span.start;
                    // Emit as import with default specifier
                    specifiers.push(ImportSpecifier::Default {
                        local: name,
                        span: Span::new(spec_start, end),
                    });
                    return Ok(Stmt::new(
                        StmtKind::Import(Box::new(ImportDecl {
                            specifiers,
                            source,
                            span: Span::new(start, end),
                            is_type_only,
                        })),
                        Span::new(start, end),
                    ));
                } else {
                    // import X = A.B.C — namespace alias, skip dotted name
                    if matches!(self.peek(), TokenKind::Identifier(_)) {
                        self.advance();
                    }
                    while self.eat(&TokenKind::Dot) {
                        if matches!(self.peek(), TokenKind::Identifier(_)) {
                            self.advance();
                        }
                    }
                    self.expect_semicolon()?;
                    let end = self.current.span.start;
                    // Type-only: strip completely
                    return Ok(Stmt::new(StmtKind::Empty, Span::new(start, end)));
                }
            }

            let spec_end = self.current.span.start;
            specifiers.push(ImportSpecifier::Default {
                local: name,
                span: Span::new(spec_start, spec_end),
            });

            if self.eat(&TokenKind::Comma) {
                // Continue to namespace or named imports
            } else {
                // Just default import
                self.expect(&TokenKind::From)?;
                let source = self.expect_string()?;
                self.consume_import_assertions();
                self.expect_semicolon()?;
                let end = self.current.span.start;
                return Ok(Stmt::new(
                    StmtKind::Import(Box::new(ImportDecl {
                        specifiers,
                        source,
                        span: Span::new(start, end),
                        #[cfg(feature = "typescript")]
                        is_type_only,
                    })),
                    Span::new(start, end),
                ));
            }
        }

        // Namespace import: * as name
        if self.eat(&TokenKind::Star) {
            self.expect(&TokenKind::As)?;
            if let TokenKind::Identifier(name) = self.peek() {
                let name = name.clone();
                let spec_start = self.current.span.start;
                self.advance();
                let spec_end = self.current.span.start;
                specifiers.push(ImportSpecifier::Namespace {
                    local: name,
                    span: Span::new(spec_start, spec_end),
                });
            } else {
                return Err(ParseError::new("Expected identifier", self.current.span));
            }
        }

        // Named imports: { a, b as c }
        if self.eat(&TokenKind::LBrace) {
            while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                let spec_start = self.current.span.start;

                #[cfg(feature = "typescript")]
                let is_type = if self.options.typescript && self.check(&TokenKind::Type) {
                    // `{ type Foo }` — but `type` could also be an imported name
                    // If followed by `,` or `}`, it's the identifier `type`, not a modifier
                    let next = self.lexer.peek();
                    match &next.kind {
                        TokenKind::Comma | TokenKind::RBrace => false,
                        _ => {
                            self.advance();
                            true
                        }
                    }
                } else {
                    false
                };

                let imported = self.expect_identifier()?;
                let local = if self.eat(&TokenKind::As) {
                    self.expect_identifier()?
                } else {
                    imported.clone()
                };
                let spec_end = self.current.span.start;

                specifiers.push(ImportSpecifier::Named {
                    imported,
                    local,
                    span: Span::new(spec_start, spec_end),
                    #[cfg(feature = "typescript")]
                    is_type,
                });

                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RBrace)?;
        }

        self.expect(&TokenKind::From)?;
        let source = self.expect_string()?;
        self.consume_import_assertions();
        self.expect_semicolon()?;
        let end = self.current.span.start;

        Ok(Stmt::new(
            StmtKind::Import(Box::new(ImportDecl {
                specifiers,
                source,
                span: Span::new(start, end),
                #[cfg(feature = "typescript")]
                is_type_only,
            })),
            Span::new(start, end),
        ))
    }

    /// Consume import assertions: `with { type: "json" }` or `assert { type: "json" }`
    fn consume_import_assertions(&mut self) {
        if self.check(&TokenKind::With) || matches!(self.peek(), TokenKind::Identifier(ref n) if n == "assert") {
            self.advance(); // consume `with` or `assert`
            if self.eat(&TokenKind::LBrace) {
                while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                    // key: value pairs
                    self.advance(); // key (identifier or keyword)
                    let _ = self.eat(&TokenKind::Colon);
                    self.advance(); // value (string)
                    self.eat(&TokenKind::Comma);
                }
                let _ = self.eat(&TokenKind::RBrace);
            }
        }
    }

    pub(crate) fn parse_export_decl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Export)?;

        #[cfg(feature = "typescript")]
        let is_type_only = if self.options.typescript && self.check(&TokenKind::Type) {
            // `export type { ... }` or `export type * from ...` — type-only re-exports
            // Do NOT consume `type` for `export type Name = ...` (that's a type alias declaration)
            let next = self.lexer.peek();
            match &next.kind {
                TokenKind::LBrace | TokenKind::Star => {
                    self.advance();
                    true
                }
                _ => false,
            }
        } else {
            false
        };

        // TypeScript: export = expr
        #[cfg(feature = "typescript")]
        if self.options.typescript && self.eat(&TokenKind::Eq) {
            let expr = self.parse_assign_expr()?;
            self.expect_semicolon()?;
            let end = self.current.span.start;
            // Treat as export default
            return Ok(Stmt::new(
                StmtKind::Export(Box::new(ExportDecl::Default {
                    expr,
                    span: Span::new(start, end),
                })),
                Span::new(start, end),
            ));
        }

        // TypeScript: export import X = ...
        #[cfg(feature = "typescript")]
        if self.options.typescript && self.check(&TokenKind::Import) {
            let decl = self.parse_import_decl()?;
            let end = self.current.span.start;
            return Ok(Stmt::new(
                StmtKind::Export(Box::new(ExportDecl::Decl {
                    decl,
                    span: Span::new(start, end),
                })),
                Span::new(start, end),
            ));
        }

        // TypeScript: export as namespace Foo;
        #[cfg(feature = "typescript")]
        if self.options.typescript && self.check(&TokenKind::As) {
            self.advance(); // eat `as`
            // expect `namespace` identifier
            if matches!(self.peek(), TokenKind::Namespace) {
                self.advance(); // eat `namespace`
            }
            let _ = self.expect_identifier()?;
            self.expect_semicolon()?;
            let end = self.current.span.start;
            return Ok(Stmt::new(
                StmtKind::Export(Box::new(ExportDecl::All {
                    exported: None,
                    source: String::new(),
                    span: Span::new(start, end),
                })),
                Span::new(start, end),
            ));
        }

        // export default
        if self.eat(&TokenKind::Default) {
            // TypeScript: export default interface Foo {}
            #[cfg(feature = "typescript")]
            if self.options.typescript && self.check(&TokenKind::Interface) {
                let decl = self.parse_ts_interface()?;
                let end = self.current.span.start;
                return Ok(Stmt::new(
                    StmtKind::Export(Box::new(ExportDecl::Decl {
                        decl,
                        span: Span::new(start, end),
                    })),
                    Span::new(start, end),
                ));
            }
            // TypeScript: export default abstract class Foo {}
            #[cfg(feature = "typescript")]
            if self.options.typescript && self.check(&TokenKind::Abstract) {
                // consume abstract, then parse class
                self.advance();
                let decl = self.parse_class_decl()?;
                let end = self.current.span.start;
                return Ok(Stmt::new(
                    StmtKind::Export(Box::new(ExportDecl::Decl {
                        decl,
                        span: Span::new(start, end),
                    })),
                    Span::new(start, end),
                ));
            }
            let expr = self.parse_assign_expr()?;
            self.expect_semicolon()?;
            let end = self.current.span.start;
            return Ok(Stmt::new(
                StmtKind::Export(Box::new(ExportDecl::Default {
                    expr,
                    span: Span::new(start, end),
                })),
                Span::new(start, end),
            ));
        }

        // export *
        if self.eat(&TokenKind::Star) {
            let exported = if self.eat(&TokenKind::As) {
                Some(self.expect_identifier()?)
            } else {
                None
            };
            self.expect(&TokenKind::From)?;
            let source = self.expect_string()?;
            self.expect_semicolon()?;
            let end = self.current.span.start;
            return Ok(Stmt::new(
                StmtKind::Export(Box::new(ExportDecl::All {
                    exported,
                    source,
                    span: Span::new(start, end),
                })),
                Span::new(start, end),
            ));
        }

        // export { ... }
        if self.check(&TokenKind::LBrace) {
            self.advance();
            let mut specifiers = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                let spec_start = self.current.span.start;

                #[cfg(feature = "typescript")]
                let is_type = if self.options.typescript && self.check(&TokenKind::Type) {
                    let next = self.lexer.peek();
                    match &next.kind {
                        TokenKind::Comma | TokenKind::RBrace => false,
                        _ => {
                            self.advance();
                            true
                        }
                    }
                } else {
                    false
                };

                let local = self.expect_identifier()?;
                let exported = if self.eat(&TokenKind::As) {
                    self.expect_identifier()?
                } else {
                    local.clone()
                };
                let spec_end = self.current.span.start;

                specifiers.push(ExportSpecifier {
                    local,
                    exported,
                    span: Span::new(spec_start, spec_end),
                    #[cfg(feature = "typescript")]
                    is_type,
                });

                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RBrace)?;

            let source = if self.eat(&TokenKind::From) {
                Some(self.expect_string()?)
            } else {
                None
            };

            self.expect_semicolon()?;
            let end = self.current.span.start;

            return Ok(Stmt::new(
                StmtKind::Export(Box::new(ExportDecl::Named {
                    specifiers,
                    source,
                    span: Span::new(start, end),
                    #[cfg(feature = "typescript")]
                    is_type_only,
                })),
                Span::new(start, end),
            ));
        }

        // export declaration (function, class, var, let, const)
        let decl = self.parse_stmt()?;
        let end = self.current.span.start;
        Ok(Stmt::new(
            StmtKind::Export(Box::new(ExportDecl::Decl {
                decl,
                span: Span::new(start, end),
            })),
            Span::new(start, end),
        ))
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        if let TokenKind::Identifier(name) = self.peek() {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            // TypeScript contextual keywords can be used as identifiers
            #[cfg(feature = "typescript")]
            if self.options.typescript {
                return self.expect_ts_identifier();
            }
            // Keywords that can appear as identifiers in import/export specifiers
            if let Some(name) = self.try_keyword_as_identifier() {
                return Ok(name);
            }
            Err(ParseError::new(
                format!("Expected identifier, got {:?}", self.peek()),
                self.current.span,
            ))
        }
    }

    /// Try to consume a keyword token and return it as an identifier string.
    /// Used in import/export specifiers where keywords like `default` are valid names.
    fn try_keyword_as_identifier(&mut self) -> Option<String> {
        let name = keyword_to_str(self.peek());
        if !name.is_empty() {
            let s = name.to_string();
            self.advance();
            Some(s)
        } else {
            None
        }
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        if let TokenKind::String(s) = self.peek() {
            let s = s.clone();
            self.advance();
            Ok(s)
        } else {
            Err(ParseError::new(
                format!("Expected string, got {:?}", self.peek()),
                self.current.span,
            ))
        }
    }

    /// Parse expression statement.
    fn parse_expr_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        let expr = self.parse_expr()?;
        self.expect_semicolon()?;
        let end = self.current.span.start;
        Ok(Stmt::new(StmtKind::Expr(expr), Span::new(start, end)))
    }

    // =========================================================================
    // Expression Parsing (Pratt Parser)
    // =========================================================================

    /// Parse an expression (with comma operator).
    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;
        let mut expr = self.parse_assign_expr()?;

        while self.eat(&TokenKind::Comma) {
            let right = self.parse_assign_expr()?;
            let end = self.current.span.start;
            expr = Expr::new(
                ExprKind::Sequence(vec![expr, right]),
                Span::new(start, end),
            );
        }

        Ok(expr)
    }

    /// Parse an assignment expression.
    pub(crate) fn parse_assign_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;

        // Single-param arrow: `x => expr` or `async x => expr`
        if let TokenKind::Identifier(ref name) = self.peek().clone() {
            if matches!(self.lexer.peek().kind, TokenKind::Arrow) {
                let name = name.clone();
                self.advance(); // eat identifier
                self.advance(); // eat =>
                let param = Param {
                    binding: Binding::new(
                        BindingKind::Ident {
                            name,
                            #[cfg(feature = "typescript")]
                            type_ann: None,
                        },
                        Span::new(start, self.current.span.start),
                    ),
                    default: None,
                    rest: false,
                    span: Span::new(start, self.current.span.start),
                };
                return self.parse_arrow_body(vec![param], false, start);
            }
        }

        let left = self.parse_conditional_expr()?;

        // Check for assignment operator
        if let Some(op) = self.get_assign_op() {
            self.advance();
            let right = self.parse_assign_expr()?;
            let end = self.current.span.start;
            return Ok(Expr::new(
                ExprKind::Assign {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                Span::new(start, end),
            ));
        }

        Ok(left)
    }

    fn get_assign_op(&self) -> Option<AssignOp> {
        match self.peek() {
            TokenKind::Eq => Some(AssignOp::Assign),
            TokenKind::PlusEq => Some(AssignOp::AddAssign),
            TokenKind::MinusEq => Some(AssignOp::SubAssign),
            TokenKind::StarEq => Some(AssignOp::MulAssign),
            TokenKind::SlashEq => Some(AssignOp::DivAssign),
            TokenKind::PercentEq => Some(AssignOp::ModAssign),
            TokenKind::StarStarEq => Some(AssignOp::PowAssign),
            TokenKind::LtLtEq => Some(AssignOp::ShlAssign),
            TokenKind::GtGtEq => Some(AssignOp::ShrAssign),
            TokenKind::GtGtGtEq => Some(AssignOp::UShrAssign),
            TokenKind::PipeEq => Some(AssignOp::BitOrAssign),
            TokenKind::CaretEq => Some(AssignOp::BitXorAssign),
            TokenKind::AmpEq => Some(AssignOp::BitAndAssign),
            TokenKind::AmpAmpEq => Some(AssignOp::AndAssign),
            TokenKind::PipePipeEq => Some(AssignOp::OrAssign),
            TokenKind::QuestionQuestionEq => Some(AssignOp::NullishAssign),
            _ => None,
        }
    }

    /// Parse conditional expression (ternary).
    fn parse_conditional_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;
        let test = self.parse_binary_expr(0)?;

        if self.eat(&TokenKind::Question) {
            let consequent = self.parse_assign_expr()?;
            self.expect(&TokenKind::Colon)?;
            let alternate = self.parse_assign_expr()?;
            let end = self.current.span.start;
            return Ok(Expr::new(
                ExprKind::Conditional {
                    test: Box::new(test),
                    consequent: Box::new(consequent),
                    alternate: Box::new(alternate),
                },
                Span::new(start, end),
            ));
        }

        Ok(test)
    }

    /// Parse binary expression using precedence climbing.
    fn parse_binary_expr(&mut self, min_prec: u8) -> Result<Expr, ParseError> {
        let start = self.current.span.start;
        let mut left = self.parse_unary_expr()?;

        loop {
            // TypeScript: `as` and `satisfies` postfix type operators
            #[cfg(feature = "typescript")]
            if self.options.typescript {
                if self.check(&TokenKind::As) {
                    self.advance();
                    // Handle `as const` — treat as a type assertion with a dummy type
                    let ty = if self.check(&TokenKind::Const) {
                        let s = self.current.span;
                        self.advance();
                        TsType { kind: TsTypeKind::Unknown, span: s }
                    } else {
                        self.parse_ts_type()?
                    };
                    let end = self.current.span.start;
                    left = Expr::new(
                        ExprKind::TsAs {
                            expr: Box::new(left),
                            ty: Box::new(ty),
                        },
                        Span::new(start, end),
                    );
                    continue;
                }
                if self.check(&TokenKind::Satisfies) {
                    self.advance();
                    let ty = self.parse_ts_type()?;
                    let end = self.current.span.start;
                    left = Expr::new(
                        ExprKind::TsSatisfies {
                            expr: Box::new(left),
                            ty: Box::new(ty),
                        },
                        Span::new(start, end),
                    );
                    continue;
                }
            }

            let op = match self.peek().binary_precedence() {
                Some(prec) if prec >= min_prec => {
                    match self.get_binary_op() {
                        Some(op) => op,
                        None => break, // e.g., `in` when allow_in is false
                    }
                }
                _ => break,
            };

            let prec = self.peek().binary_precedence().unwrap();
            let is_right_assoc = self.peek().is_right_associative();
            self.advance();

            let next_prec = if is_right_assoc { prec } else { prec + 1 };
            let right = self.parse_binary_expr(next_prec)?;
            let end = self.current.span.start;

            left = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                Span::new(start, end),
            );
        }

        Ok(left)
    }

    fn get_binary_op(&self) -> Option<BinaryOp> {
        match self.peek() {
            TokenKind::Plus => Some(BinaryOp::Add),
            TokenKind::Minus => Some(BinaryOp::Sub),
            TokenKind::Star => Some(BinaryOp::Mul),
            TokenKind::Slash => Some(BinaryOp::Div),
            TokenKind::Percent => Some(BinaryOp::Mod),
            TokenKind::StarStar => Some(BinaryOp::Pow),
            TokenKind::EqEq => Some(BinaryOp::Eq),
            TokenKind::BangEq => Some(BinaryOp::NotEq),
            TokenKind::EqEqEq => Some(BinaryOp::StrictEq),
            TokenKind::BangEqEq => Some(BinaryOp::StrictNotEq),
            TokenKind::Lt => Some(BinaryOp::Lt),
            TokenKind::LtEq => Some(BinaryOp::LtEq),
            TokenKind::Gt => Some(BinaryOp::Gt),
            TokenKind::GtEq => Some(BinaryOp::GtEq),
            TokenKind::Pipe => Some(BinaryOp::BitOr),
            TokenKind::Caret => Some(BinaryOp::BitXor),
            TokenKind::Amp => Some(BinaryOp::BitAnd),
            TokenKind::LtLt => Some(BinaryOp::Shl),
            TokenKind::GtGt => Some(BinaryOp::Shr),
            TokenKind::GtGtGt => Some(BinaryOp::UShr),
            TokenKind::AmpAmp => Some(BinaryOp::And),
            TokenKind::PipePipe => Some(BinaryOp::Or),
            TokenKind::QuestionQuestion => Some(BinaryOp::NullishCoalesce),
            TokenKind::In if self.allow_in => Some(BinaryOp::In),
            TokenKind::Instanceof => Some(BinaryOp::Instanceof),
            _ => None,
        }
    }

    /// Parse unary expression.
    pub(crate) fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;

        // Prefix unary operators
        let op = match self.peek() {
            TokenKind::Minus => Some(UnaryOp::Minus),
            TokenKind::Plus => Some(UnaryOp::Plus),
            TokenKind::Bang => Some(UnaryOp::Not),
            TokenKind::Tilde => Some(UnaryOp::BitNot),
            TokenKind::Typeof => Some(UnaryOp::Typeof),
            TokenKind::Void => Some(UnaryOp::Void),
            TokenKind::Delete => Some(UnaryOp::Delete),
            _ => None,
        };

        if let Some(op) = op {
            self.advance();
            let arg = self.parse_unary_expr()?;
            let end = self.current.span.start;
            return Ok(Expr::new(
                ExprKind::Unary {
                    op,
                    arg: Box::new(arg),
                },
                Span::new(start, end),
            ));
        }

        // Prefix update operators
        if matches!(self.peek(), TokenKind::PlusPlus | TokenKind::MinusMinus) {
            let op = if self.check(&TokenKind::PlusPlus) {
                UpdateOp::Increment
            } else {
                UpdateOp::Decrement
            };
            self.advance();
            let arg = self.parse_unary_expr()?;
            let end = self.current.span.start;
            return Ok(Expr::new(
                ExprKind::Update {
                    op,
                    prefix: true,
                    arg: Box::new(arg),
                },
                Span::new(start, end),
            ));
        }

        // Await expression
        if self.eat(&TokenKind::Await) {
            let arg = self.parse_unary_expr()?;
            let end = self.current.span.start;
            return Ok(Expr::new(
                ExprKind::Await(Box::new(arg)),
                Span::new(start, end),
            ));
        }

        self.parse_postfix_expr()
    }

    /// Parse postfix expression.
    fn parse_postfix_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;
        let mut expr = self.parse_left_hand_side_expr()?;

        // TypeScript: non-null assertion `x!` (only if no preceding newline)
        #[cfg(feature = "typescript")]
        if self.options.typescript && self.check(&TokenKind::Bang) && !self.current.had_newline_before {
            self.advance();
            let end = self.current.span.start;
            expr = Expr::new(
                ExprKind::TsNonNull(Box::new(expr)),
                Span::new(start, end),
            );
        }

        // Postfix update operators
        if matches!(self.peek(), TokenKind::PlusPlus | TokenKind::MinusMinus) {
            let op = if self.check(&TokenKind::PlusPlus) {
                UpdateOp::Increment
            } else {
                UpdateOp::Decrement
            };
            self.advance();
            let end = self.current.span.start;
            expr = Expr::new(
                ExprKind::Update {
                    op,
                    prefix: false,
                    arg: Box::new(expr),
                },
                Span::new(start, end),
            );
        }

        Ok(expr)
    }

    /// Parse left-hand-side expression (call, member access).
    pub(crate) fn parse_left_hand_side_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;

        // new expression
        if self.eat(&TokenKind::New) {
            // new.target meta property
            if self.eat(&TokenKind::Dot) {
                if let TokenKind::Identifier(name) = self.peek() {
                    let name = name.clone();
                    self.advance();
                    let end = self.current.span.start;
                    return Ok(Expr::new(
                        ExprKind::MetaProperty {
                            meta: "new".to_string(),
                            property: name,
                        },
                        Span::new(start, end),
                    ));
                }
            }
            let callee = self.parse_left_hand_side_expr()?;
            // TypeScript: consume type arguments on new expression (e.g., `new Map<K, V>()`)
            #[cfg(feature = "typescript")]
            if self.options.typescript && self.check(&TokenKind::Lt) {
                let _ = self.parse_ts_type_args_impl()?;
            }
            let args = if self.check(&TokenKind::LParen) {
                self.parse_arguments()?
            } else {
                Vec::new()
            };
            let end = self.current.span.start;
            return Ok(Expr::new(
                ExprKind::New {
                    callee: Box::new(callee),
                    args,
                },
                Span::new(start, end),
            ));
        }

        let mut expr = self.parse_primary_expr()?;

        loop {
            match self.peek() {
                // Member access: a.b
                TokenKind::Dot => {
                    self.advance();
                    let property = self.parse_member_property()?;
                    let end = self.current.span.start;
                    expr = Expr::new(
                        ExprKind::Member {
                            object: Box::new(expr),
                            property: Box::new(property),
                            computed: false,
                        },
                        Span::new(start, end),
                    );
                }
                // Computed member access: a[b]
                TokenKind::LBracket => {
                    self.advance();
                    let property = self.parse_expr()?;
                    self.expect(&TokenKind::RBracket)?;
                    let end = self.current.span.start;
                    expr = Expr::new(
                        ExprKind::Member {
                            object: Box::new(expr),
                            property: Box::new(property),
                            computed: true,
                        },
                        Span::new(start, end),
                    );
                }
                // Optional chaining: a?.b or a?.[b] or a?.()
                TokenKind::QuestionDot => {
                    self.advance();
                    if self.check(&TokenKind::LBracket) {
                        self.advance();
                        let property = self.parse_expr()?;
                        self.expect(&TokenKind::RBracket)?;
                        let end = self.current.span.start;
                        expr = Expr::new(
                            ExprKind::OptionalMember {
                                object: Box::new(expr),
                                property: Box::new(property),
                                computed: true,
                            },
                            Span::new(start, end),
                        );
                    } else if self.check(&TokenKind::LParen) {
                        let args = self.parse_arguments()?;
                        let end = self.current.span.start;
                        expr = Expr::new(
                            ExprKind::OptionalCall {
                                callee: Box::new(expr),
                                args,
                            },
                            Span::new(start, end),
                        );
                    } else {
                        let property = self.parse_member_property()?;
                        let end = self.current.span.start;
                        expr = Expr::new(
                            ExprKind::OptionalMember {
                                object: Box::new(expr),
                                property: Box::new(property),
                                computed: false,
                            },
                            Span::new(start, end),
                        );
                    }
                }
                // TypeScript: type args before call: a<T>(b) or instantiation: a<T>
                #[cfg(feature = "typescript")]
                TokenKind::Lt if self.options.typescript => {
                    if self.try_parse_ts_type_args_for_call() {
                        // Type args consumed; if followed by `(` parse call, otherwise it's an instantiation expression
                        if self.check(&TokenKind::LParen) {
                            expr = self.parse_call_expr(expr)?;
                        }
                        // For instantiation expressions, type args are stripped; continue the loop
                    } else {
                        break;
                    }
                }
                // Function call: a(b)
                TokenKind::LParen => {
                    expr = self.parse_call_expr(expr)?;
                }
                // Template literal: a`template`
                TokenKind::TemplateNoSub(_) | TokenKind::TemplateHead(_) => {
                    let quasi = self.parse_template_literal()?;
                    let end = self.current.span.start;
                    expr = Expr::new(
                        ExprKind::TaggedTemplate {
                            tag: Box::new(expr),
                            quasi: Box::new(quasi),
                        },
                        Span::new(start, end),
                    );
                }
                // TypeScript: non-null assertion `expr!` (no newline before)
                #[cfg(feature = "typescript")]
                TokenKind::Bang if self.options.typescript && !self.current.had_newline_before => {
                    self.advance();
                    // Just strip the `!` — expr is already the inner expression
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    /// Parse a member property after `.` — any identifier or keyword is valid as a property name.
    fn parse_member_property(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;
        // After `.`, any keyword or identifier is a valid property name
        if let TokenKind::Identifier(name) = self.peek() {
            let name = name.clone();
            self.advance();
            Ok(Expr::new(ExprKind::Ident(name), Span::new(start, self.current.span.start)))
        } else if self.peek().is_keyword() {
            let name = keyword_to_str(self.peek()).to_string();
            self.advance();
            Ok(Expr::new(ExprKind::Ident(name), Span::new(start, self.current.span.start)))
        } else if self.check(&TokenKind::Hash) {
            // Private field: .#name
            self.advance();
            if let TokenKind::Identifier(n) = self.peek() {
                let name = n.clone();
                self.advance();
                let end = self.current.span.start;
                Ok(Expr::new(ExprKind::Ident(format!("#{}", name)), Span::new(start, end)))
            } else {
                Err(ParseError::new("Expected identifier after '#'", self.current.span))
            }
        } else {
            // TypeScript contextual keywords
            #[cfg(feature = "typescript")]
            if self.options.typescript && crate::typescript::is_ts_contextual_keyword(self.peek()) {
                let name = self.expect_ts_identifier()?;
                return Ok(Expr::new(ExprKind::Ident(name), Span::new(start, self.current.span.start)));
            }
            Err(ParseError::new(
                format!("Expected property name, got {:?}", self.peek()),
                self.current.span,
            ))
        }
    }

    /// Parse function call.
    fn parse_call_expr(&mut self, callee: Expr) -> Result<Expr, ParseError> {
        let start = callee.span.start;
        let args = self.parse_arguments()?;
        let end = self.current.span.start;
        Ok(Expr::new(
            ExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            Span::new(start, end),
        ))
    }

    /// Parse function arguments.
    fn parse_arguments(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect(&TokenKind::LParen)?;
        let mut args = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.is_eof() {
            if self.eat(&TokenKind::Spread) {
                let arg = self.parse_assign_expr()?;
                let arg_start = arg.span.start;
                let end = self.current.span.start;
                args.push(Expr::new(
                    ExprKind::Spread(Box::new(arg)),
                    Span::new(arg_start, end),
                ));
            } else {
                args.push(self.parse_assign_expr()?);
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen)?;
        Ok(args)
    }

    /// Parse primary expression.
    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;

        match self.peek().clone() {
            // Literals
            TokenKind::Null => {
                self.advance();
                Ok(Expr::new(ExprKind::Null, Span::new(start, start + 4)))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::new(ExprKind::Bool(true), Span::new(start, start + 4)))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::new(ExprKind::Bool(false), Span::new(start, start + 5)))
            }
            TokenKind::Number(n) => {
                self.advance();
                Ok(Expr::new(ExprKind::Number(n), Span::new(start, self.current.span.start)))
            }
            TokenKind::BigInt(s) => {
                self.advance();
                Ok(Expr::new(ExprKind::BigInt(s), Span::new(start, self.current.span.start)))
            }
            TokenKind::String(s) => {
                self.advance();
                Ok(Expr::new(ExprKind::String(s), Span::new(start, self.current.span.start)))
            }
            TokenKind::Regex { pattern, flags } => {
                self.advance();
                Ok(Expr::new(ExprKind::Regex { pattern, flags }, Span::new(start, self.current.span.start)))
            }

            // Identifiers
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Expr::new(ExprKind::Ident(name), Span::new(start, self.current.span.start)))
            }
            TokenKind::This => {
                self.advance();
                Ok(Expr::new(ExprKind::This, Span::new(start, start + 4)))
            }
            TokenKind::Super => {
                self.advance();
                Ok(Expr::new(ExprKind::Super, Span::new(start, start + 5)))
            }
            // Private field access: #field (after obj.)
            TokenKind::Hash => {
                self.advance();
                let name = if let TokenKind::Identifier(n) = self.peek() {
                    let n = n.clone();
                    self.advance();
                    n
                } else {
                    return Err(ParseError::new("Expected identifier after '#'", self.current.span));
                };
                let end = self.current.span.start;
                Ok(Expr::new(ExprKind::Ident(format!("#{}", name)), Span::new(start, end)))
            }

            // Template literals
            TokenKind::TemplateNoSub(s) => {
                self.advance();
                Ok(Expr::new(ExprKind::TemplateNoSub(s), Span::new(start, self.current.span.start)))
            }
            TokenKind::TemplateHead(_) => self.parse_template_literal(),

            // Array literal
            TokenKind::LBracket => self.parse_array_literal(),

            // Object literal
            TokenKind::LBrace => self.parse_object_literal(),

            // Parenthesized expression or arrow function
            TokenKind::LParen => self.parse_paren_expr(),

            // Function expression
            TokenKind::Function => {
                let func = self.parse_function(false)?;
                let end = func.span.end;
                Ok(Expr::new(ExprKind::Function(Box::new(func)), Span::new(start, end)))
            }

            // Class expression
            TokenKind::Class => {
                let class = self.parse_class()?;
                let end = class.span.end;
                Ok(Expr::new(ExprKind::Class(Box::new(class)), Span::new(start, end)))
            }

            // Async function/arrow
            TokenKind::Async => {
                self.advance();
                if self.check(&TokenKind::Function) {
                    let func = self.parse_function(true)?;
                    let end = func.span.end;
                    Ok(Expr::new(ExprKind::Function(Box::new(func)), Span::new(start, end)))
                } else if self.check(&TokenKind::LParen) {
                    // async (...) => ... or async(...) call
                    self.parse_paren_or_arrow(true, start)
                } else if matches!(self.peek(), TokenKind::Identifier(_)) {
                    // Could be: async x => x (simple async arrow)
                    let next = self.lexer.peek();
                    if matches!(next.kind, TokenKind::Arrow) {
                        let name = match self.peek().clone() {
                            TokenKind::Identifier(n) => n,
                            _ => unreachable!(),
                        };
                        self.advance(); // eat identifier
                        self.advance(); // eat =>
                        let param = Param {
                            binding: Binding::new(
                                BindingKind::Ident {
                                    name,
                                    #[cfg(feature = "typescript")]
                                    type_ann: None,
                                },
                                Span::new(start, self.current.span.start),
                            ),
                            default: None,
                            rest: false,
                            span: Span::new(start, self.current.span.start),
                        };
                        self.parse_arrow_body(vec![param], true, start)
                    } else {
                        Ok(Expr::new(ExprKind::Ident("async".to_string()), Span::new(start, start + 5)))
                    }
                } else {
                    Ok(Expr::new(ExprKind::Ident("async".to_string()), Span::new(start, start + 5)))
                }
            }

            // Yield expression
            TokenKind::Yield => {
                self.advance();
                let delegate = self.eat(&TokenKind::Star);
                let arg = if self.peek().can_start_expr() {
                    Some(Box::new(self.parse_assign_expr()?))
                } else {
                    None
                };
                let end = self.current.span.start;
                Ok(Expr::new(ExprKind::Yield { arg, delegate }, Span::new(start, end)))
            }

            // Import expression
            TokenKind::Import => {
                self.advance();
                if self.eat(&TokenKind::LParen) {
                    let arg = self.parse_assign_expr()?;
                    // Consume optional second argument (import options)
                    if self.eat(&TokenKind::Comma) {
                        if !self.check(&TokenKind::RParen) {
                            let _ = self.parse_assign_expr()?;
                            self.eat(&TokenKind::Comma); // trailing comma
                        }
                    }
                    self.expect(&TokenKind::RParen)?;
                    let end = self.current.span.start;
                    Ok(Expr::new(ExprKind::Import(Box::new(arg)), Span::new(start, end)))
                } else if self.eat(&TokenKind::Dot) {
                    // import.meta
                    if let TokenKind::Identifier(name) = self.peek() {
                        let name = name.clone();
                        self.advance();
                        let end = self.current.span.start;
                        Ok(Expr::new(
                            ExprKind::MetaProperty {
                                meta: "import".to_string(),
                                property: name,
                            },
                            Span::new(start, end),
                        ))
                    } else {
                        Err(ParseError::new("Expected identifier after 'import.'", self.current.span))
                    }
                } else {
                    Err(ParseError::new("Expected '(' or '.' after 'import'", self.current.span))
                }
            }

            // JSX element or fragment
            #[cfg(feature = "jsx")]
            TokenKind::Lt if self.options.jsx => {
                self.parse_jsx_element_or_fragment()
            }

            // TypeScript: <T>expr type assertion or <T>(...) => expr generic arrow (.ts only, not .tsx)
            #[cfg(feature = "typescript")]
            TokenKind::Lt if self.options.typescript && !self.options.jsx => {
                self.parse_ts_angle_bracket_expr(start)
            }

            // JS contextual keywords used as identifiers in expression context
            TokenKind::Get | TokenKind::Set | TokenKind::From
            | TokenKind::Static | TokenKind::As => {
                let name = keyword_to_str(self.peek()).to_string();
                self.advance();
                Ok(Expr::new(ExprKind::Ident(name), Span::new(start, self.current.span.start)))
            }

            // TypeScript contextual keywords used as identifiers
            #[cfg(feature = "typescript")]
            _ if self.options.typescript && crate::typescript::is_ts_contextual_keyword(self.peek()) => {
                let name = self.expect_ts_identifier()?;
                Ok(Expr::new(ExprKind::Ident(name), Span::new(start, self.current.span.start)))
            }

            _ => Err(ParseError::new(
                format!("Unexpected token: {:?}", self.peek()),
                self.current.span,
            )),
        }
    }

    /// Parse array literal.
    fn parse_array_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::LBracket)?;

        let mut elements = Vec::new();
        while !self.check(&TokenKind::RBracket) && !self.is_eof() {
            if self.check(&TokenKind::Comma) {
                elements.push(None);
            } else if self.eat(&TokenKind::Spread) {
                let arg = self.parse_assign_expr()?;
                let end = self.current.span.start;
                elements.push(Some(Box::new(Expr::new(
                    ExprKind::Spread(Box::new(arg)),
                    Span::new(start, end),
                ))));
            } else {
                elements.push(Some(Box::new(self.parse_assign_expr()?)));
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        let end = self.current.span.end;
        self.expect(&TokenKind::RBracket)?;

        Ok(Expr::new(ExprKind::Array(elements), Span::new(start, end)))
    }

    /// Parse object literal.
    fn parse_object_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::LBrace)?;

        let mut properties = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            let prop_start = self.current.span.start;

            // Spread property
            if self.eat(&TokenKind::Spread) {
                let arg = self.parse_assign_expr()?;
                let end = self.current.span.start;
                properties.push(Property {
                    key: PropertyKey::Ident(String::new()),
                    value: Expr::new(ExprKind::Spread(Box::new(arg)), Span::new(prop_start, end)),
                    kind: PropertyKind::Init,
                    shorthand: false,
                    computed: false,
                    span: Span::new(prop_start, end),
                });
            } else {
                // Check for async method: { async foo() {} }
                let is_async = self.check(&TokenKind::Async)
                    && !matches!(self.lexer.peek().kind, TokenKind::LParen | TokenKind::Colon | TokenKind::Comma | TokenKind::RBrace);
                if is_async {
                    self.advance();
                }

                // Check for generator method: { *foo() {} }
                let is_generator = !is_async && self.eat(&TokenKind::Star);

                // Check for getter/setter — only when followed by a property key
                // `get name() {}` → getter; `get: val` or `get()` or `get,` → regular property
                let mut kind = PropertyKind::Init;
                if self.check(&TokenKind::Get) && !matches!(self.lexer.peek().kind, TokenKind::LParen | TokenKind::Colon | TokenKind::Comma | TokenKind::RBrace | TokenKind::Eq) {
                    self.advance();
                    kind = PropertyKind::Get;
                } else if self.check(&TokenKind::Set) && !matches!(self.lexer.peek().kind, TokenKind::LParen | TokenKind::Colon | TokenKind::Comma | TokenKind::RBrace | TokenKind::Eq) {
                    self.advance();
                    kind = PropertyKind::Set;
                }

                // Property key
                let computed = self.check(&TokenKind::LBracket);
                let key = self.parse_property_key()?;

                // Method shorthand: { foo() {} }
                if self.check(&TokenKind::LParen) || is_async || is_generator {
                    #[cfg(feature = "typescript")]
                    let type_params = if self.options.typescript && self.check(&TokenKind::Lt) {
                        Some(self.parse_ts_type_params_impl()?)
                    } else {
                        None
                    };
                    let params = self.parse_params()?;
                    #[cfg(feature = "typescript")]
                    if self.options.typescript && self.eat(&TokenKind::Colon) {
                        let _ = self.parse_ts_type()?;
                    }
                    self.expect(&TokenKind::LBrace)?;
                    let mut body = Vec::new();
                    while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                        body.push(self.parse_stmt()?);
                    }
                    let end = self.current.span.end;
                    self.expect(&TokenKind::RBrace)?;

                    let func = Function {
                        name: None,
                        params,
                        body,
                        is_async,
                        is_generator,
                        span: Span::new(prop_start, end),
                        #[cfg(feature = "typescript")]
                        type_params,
                        #[cfg(feature = "typescript")]
                        return_type: None,
                    };

                    properties.push(Property {
                        key,
                        value: Expr::new(ExprKind::Function(Box::new(func)), Span::new(prop_start, end)),
                        kind: if kind == PropertyKind::Init { PropertyKind::Method } else { kind },
                        shorthand: false,
                        computed,
                        span: Span::new(prop_start, end),
                    });
                } else if self.eat(&TokenKind::Colon) {
                    // Regular property: { key: value }
                    let value = self.parse_assign_expr()?;
                    let end = self.current.span.start;
                    properties.push(Property {
                        key,
                        value,
                        kind,
                        shorthand: false,
                        computed,
                        span: Span::new(prop_start, end),
                    });
                } else if self.eat(&TokenKind::Eq) {
                    // Shorthand property with default: { key = value } (destructuring)
                    let name = match &key {
                        PropertyKey::Ident(n) => n.clone(),
                        _ => return Err(ParseError::new(
                            "Expected identifier in shorthand property",
                            self.current.span,
                        )),
                    };
                    let default_value = self.parse_assign_expr()?;
                    let end = self.current.span.start;
                    properties.push(Property {
                        key,
                        value: Expr::new(
                            ExprKind::Assign {
                                op: AssignOp::Assign,
                                left: Box::new(Expr::new(ExprKind::Ident(name), Span::new(prop_start, end))),
                                right: Box::new(default_value),
                            },
                            Span::new(prop_start, end),
                        ),
                        kind: PropertyKind::Init,
                        shorthand: true,
                        computed: false,
                        span: Span::new(prop_start, end),
                    });
                } else {
                    // Shorthand property: { key }
                    let name = match &key {
                        PropertyKey::Ident(n) => n.clone(),
                        _ => return Err(ParseError::new(
                            "Expected identifier in shorthand property",
                            self.current.span,
                        )),
                    };
                    let end = self.current.span.start;
                    properties.push(Property {
                        key,
                        value: Expr::new(ExprKind::Ident(name), Span::new(prop_start, end)),
                        kind: PropertyKind::Init,
                        shorthand: true,
                        computed: false,
                        span: Span::new(prop_start, end),
                    });
                }
            }

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        let end = self.current.span.end;
        self.expect(&TokenKind::RBrace)?;

        Ok(Expr::new(ExprKind::Object(properties), Span::new(start, end)))
    }

    /// Parse parenthesized expression or arrow function.
    fn parse_paren_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_paren_or_arrow(false, self.current.span.start)
    }

    /// Parse parenthesized expression, arrow function, or async arrow/call.
    /// When is_async=true, `async` has already been consumed and `(` is current.
    /// If no `=>` follows and is_async=true, creates a call expression `async(...)`.
    fn parse_paren_or_arrow(&mut self, is_async: bool, outer_start: u32) -> Result<Expr, ParseError> {
        let paren_start = self.current.span.start;
        self.expect(&TokenKind::LParen)?;

        // === Empty parens ===
        if self.check(&TokenKind::RParen) {
            self.advance();
            // TypeScript: return type annotation
            #[cfg(feature = "typescript")]
            if self.options.typescript && self.eat(&TokenKind::Colon) {
                let _ = self.parse_ts_type()?;
            }
            if self.check(&TokenKind::Arrow) {
                self.advance();
                return self.parse_arrow_body(Vec::new(), is_async, outer_start);
            }
            if is_async {
                let callee = Expr::new(ExprKind::Ident("async".to_string()), Span::new(outer_start, paren_start));
                let end = self.current.span.start;
                return Ok(Expr::new(ExprKind::Call { callee: Box::new(callee), args: vec![] }, Span::new(outer_start, end)));
            }
            return Err(ParseError::new("Expected =>", self.current.span));
        }

        // === TypeScript: detect typed arrow params via lookahead ===
        #[cfg(feature = "typescript")]
        if self.options.typescript {
            let is_typed = {
                let kind = self.peek().clone();
                match kind {
                    TokenKind::Identifier(_) | TokenKind::This => {
                        let next = self.lexer.peek();
                        match &next.kind {
                            TokenKind::Colon => true,
                            TokenKind::Question => {
                                // Disambiguate (x?: type) from (x ? expr : expr)
                                // Peek 3rd token: if Colon/RParen/Comma → typed param
                                let saved = self.lexer.clone();
                                let _ = self.lexer.next_token(); // skip past Question
                                let third = self.lexer.peek();
                                let result = matches!(third.kind, TokenKind::Colon | TokenKind::RParen | TokenKind::Comma);
                                self.lexer = saved;
                                result
                            }
                            _ => false,
                        }
                    }
                    TokenKind::Spread => true,
                    _ if kind.is_keyword() || crate::typescript::is_ts_contextual_keyword(&kind) => {
                        let next = self.lexer.peek();
                        match &next.kind {
                            TokenKind::Colon => true,
                            TokenKind::Question => {
                                let saved = self.lexer.clone();
                                let _ = self.lexer.next_token();
                                let third = self.lexer.peek();
                                let result = matches!(third.kind, TokenKind::Colon | TokenKind::RParen | TokenKind::Comma);
                                self.lexer = saved;
                                result
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                }
            };
            if is_typed {
                let params = self.parse_params_inner()?;
                self.expect(&TokenKind::RParen)?;
                if self.eat(&TokenKind::Colon) {
                    let _ = self.parse_ts_type()?;
                }
                self.expect(&TokenKind::Arrow)?;
                return self.parse_arrow_body(params, is_async, outer_start);
            }
        }

        // === Parse first expression ===
        let first = self.parse_assign_expr()?;

        // TypeScript: colon after expression → must be typed arrow params
        #[cfg(feature = "typescript")]
        if self.options.typescript && self.check(&TokenKind::Colon) {
            return self.finish_ts_arrow_from_exprs(vec![first], is_async, outer_start);
        }

        // === Comma → sequence expression or arrow params ===
        if self.eat(&TokenKind::Comma) {
            let mut exprs = vec![first];
            while !self.check(&TokenKind::RParen) && !self.is_eof() {
                // Rest param → must be arrow function
                if self.check(&TokenKind::Spread) {
                    let mut params: Vec<Param> = Vec::new();
                    for expr in exprs {
                        params.push(self.expr_to_param(expr)?);
                    }
                    let rest_start = self.current.span.start;
                    self.advance(); // eat ...
                    let binding = self.parse_binding()?;
                    #[cfg(feature = "typescript")]
                    if self.options.typescript {
                        self.eat(&TokenKind::Question);
                    }
                    let rest_end = self.current.span.start;
                    params.push(Param { binding, default: None, rest: true, span: Span::new(rest_start, rest_end) });
                    self.expect(&TokenKind::RParen)?;
                    #[cfg(feature = "typescript")]
                    if self.options.typescript && self.eat(&TokenKind::Colon) {
                        let _ = self.parse_ts_type()?;
                    }
                    self.expect(&TokenKind::Arrow)?;
                    return self.parse_arrow_body(params, is_async, outer_start);
                }

                exprs.push(self.parse_assign_expr()?);

                // TypeScript: colon after expression in comma list → typed arrow
                #[cfg(feature = "typescript")]
                if self.options.typescript && self.check(&TokenKind::Colon) {
                    return self.finish_ts_arrow_from_exprs(exprs, is_async, outer_start);
                }

                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen)?;

            // TypeScript: return type annotation — use backtracking
            #[cfg(feature = "typescript")]
            if self.options.typescript && self.check(&TokenKind::Colon) {
                let saved_lexer = self.lexer.clone();
                let saved_token = self.current.clone();
                self.advance();
                let type_ok = self.parse_ts_type().is_ok();
                if type_ok && self.check(&TokenKind::Arrow) {
                    self.advance();
                    let params = self.exprs_to_params(exprs)?;
                    return self.parse_arrow_body(params, is_async, outer_start);
                }
                self.lexer = saved_lexer;
                self.current = saved_token;
            }

            if self.check(&TokenKind::Arrow) {
                self.advance();
                let params = self.exprs_to_params(exprs)?;
                return self.parse_arrow_body(params, is_async, outer_start);
            }

            if is_async {
                let callee = Expr::new(ExprKind::Ident("async".to_string()), Span::new(outer_start, paren_start));
                let end = self.current.span.start;
                return Ok(Expr::new(ExprKind::Call { callee: Box::new(callee), args: exprs }, Span::new(outer_start, end)));
            }
            let end = self.current.span.start;
            return Ok(Expr::new(ExprKind::Sequence(exprs), Span::new(outer_start, end)));
        }

        // === Single expression, no comma ===
        self.expect(&TokenKind::RParen)?;

        // TypeScript: return type annotation before arrow
        // Ambiguity: `cond ? (x) : value` (ternary) vs `(x): string => x` (arrow with return type)
        // Use backtracking: try parsing as return type + arrow, restore if no arrow follows
        #[cfg(feature = "typescript")]
        if self.options.typescript && self.check(&TokenKind::Colon)
            && matches!(&first.kind, ExprKind::Ident(_) | ExprKind::Assign { .. } | ExprKind::Array(_) | ExprKind::Object(_))
        {
            let saved_lexer = self.lexer.clone();
            let saved_token = self.current.clone();
            self.advance(); // consume ':'
            let type_ok = self.parse_ts_type().is_ok();
            if type_ok && self.check(&TokenKind::Arrow) {
                self.advance();
                let params = self.exprs_to_params(vec![first])?;
                return self.parse_arrow_body(params, is_async, outer_start);
            }
            // Not an arrow — restore lexer state
            self.lexer = saved_lexer;
            self.current = saved_token;
        }

        if self.check(&TokenKind::Arrow) {
            self.advance();
            let params = self.exprs_to_params(vec![first])?;
            return self.parse_arrow_body(params, is_async, outer_start);
        }

        if is_async {
            let callee = Expr::new(ExprKind::Ident("async".to_string()), Span::new(outer_start, paren_start));
            let end = self.current.span.start;
            return Ok(Expr::new(ExprKind::Call { callee: Box::new(callee), args: vec![first] }, Span::new(outer_start, end)));
        }

        // Just a parenthesized expression
        Ok(first)
    }

    /// Convert expressions to arrow function parameters.
    fn exprs_to_params(&self, exprs: Vec<Expr>) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        for expr in exprs {
            let param = self.expr_to_param(expr)?;
            params.push(param);
        }
        Ok(params)
    }

    /// Convert an expression to a parameter.
    fn expr_to_param(&self, expr: Expr) -> Result<Param, ParseError> {
        match expr.kind {
            ExprKind::Ident(name) => Ok(Param {
                binding: Binding::new(
                    BindingKind::Ident {
                        name,
                        #[cfg(feature = "typescript")]
                        type_ann: None,
                    },
                    expr.span,
                ),
                default: None,
                rest: false,
                span: expr.span,
            }),
            ExprKind::Assign { left, right, op: AssignOp::Assign } => {
                let binding = self.expr_to_binding(*left)?;
                Ok(Param {
                    binding,
                    default: Some(*right),
                    rest: false,
                    span: expr.span,
                })
            }
            ExprKind::Spread(arg) => {
                let binding = self.expr_to_binding(*arg)?;
                Ok(Param {
                    binding,
                    default: None,
                    rest: true,
                    span: expr.span,
                })
            }
            ExprKind::Object(_) | ExprKind::Array(_) => {
                let binding = self.expr_to_binding(expr.clone())?;
                Ok(Param {
                    binding,
                    default: None,
                    rest: false,
                    span: expr.span,
                })
            }
            _ => Err(ParseError::new(
                "Invalid arrow function parameter",
                expr.span,
            )),
        }
    }

    /// Convert an expression to a binding pattern.
    fn expr_to_binding(&self, expr: Expr) -> Result<Binding, ParseError> {
        let span = expr.span;
        match expr.kind {
            ExprKind::Ident(name) => Ok(Binding::new(
                BindingKind::Ident {
                    name,
                    #[cfg(feature = "typescript")]
                    type_ann: None,
                },
                span,
            )),
            ExprKind::Object(props) => {
                let mut bindings = Vec::new();
                for prop in props {
                    if prop.kind == PropertyKind::Method || prop.kind == PropertyKind::Get || prop.kind == PropertyKind::Set {
                        return Err(ParseError::new("Invalid binding in object pattern", span));
                    }
                    if prop.shorthand {
                        // { a } → single binding
                        if let PropertyKey::Ident(name) = prop.key {
                            bindings.push(ObjectPatternProperty {
                                key: PropertyKey::Ident(name.clone()),
                                value: Binding::new(BindingKind::Ident {
                                    name,
                                    #[cfg(feature = "typescript")]
                                    type_ann: None,
                                }, prop.span),
                                default: None,
                                shorthand: true,
                                rest: false,
                            });
                        }
                    } else {
                        // { a: b } → key-value binding
                        let binding = self.expr_to_binding(prop.value)?;
                        bindings.push(ObjectPatternProperty {
                            key: prop.key,
                            value: binding,
                            default: None,
                            shorthand: false,
                            rest: false,
                        });
                    }
                }
                Ok(Binding::new(BindingKind::Object {
                    properties: bindings,
                    #[cfg(feature = "typescript")]
                    type_ann: None,
                }, span))
            }
            ExprKind::Spread(inner) => {
                // Used in object spread → rest binding
                self.expr_to_binding(*inner)
            }
            ExprKind::Array(elems) => {
                let mut bindings = Vec::new();
                for elem in elems {
                    match elem {
                        Some(e) => {
                            match e.kind {
                                ExprKind::Spread(inner) => {
                                    let binding = self.expr_to_binding(*inner)?;
                                    bindings.push(Some(ArrayPatternElement { binding, default: None, rest: true }));
                                }
                                ExprKind::Assign { left, right, op: AssignOp::Assign } => {
                                    let binding = self.expr_to_binding(*left)?;
                                    bindings.push(Some(ArrayPatternElement { binding, default: Some(*right), rest: false }));
                                }
                                _ => {
                                    let binding = self.expr_to_binding(*e)?;
                                    bindings.push(Some(ArrayPatternElement { binding, default: None, rest: false }));
                                }
                            }
                        }
                        None => bindings.push(None),
                    }
                }
                Ok(Binding::new(BindingKind::Array {
                    elements: bindings,
                    #[cfg(feature = "typescript")]
                    type_ann: None,
                }, span))
            }
            ExprKind::Assign { left, right: _, op: AssignOp::Assign } => {
                // `x = default` inside destructuring
                self.expr_to_binding(*left)
            }
            _ => Err(ParseError::new(
                "Invalid binding pattern",
                span,
            )),
        }
    }

    /// Convert already-parsed expressions to typed arrow function parameters.
    /// Called when we encounter `:` after an expression inside `()` in TypeScript mode.
    /// The last expression in `exprs` is at `:`, all prior ones are untyped params.
    #[cfg(feature = "typescript")]
    fn finish_ts_arrow_from_exprs(&mut self, exprs: Vec<Expr>, is_async: bool, start: u32) -> Result<Expr, ParseError> {
        let mut params = Vec::new();
        let last_idx = exprs.len() - 1;

        for (i, expr) in exprs.into_iter().enumerate() {
            if i == last_idx && self.check(&TokenKind::Colon) {
                // Last expr has a type annotation
                let binding = self.expr_to_binding(expr.clone())?;
                self.advance(); // eat :
                let type_ann = Some(Box::new(self.parse_ts_type()?));
                let binding = match binding.kind {
                    BindingKind::Ident { name, .. } => Binding::new(
                        BindingKind::Ident { name, type_ann },
                        binding.span,
                    ),
                    other => Binding::new(other, binding.span),
                };
                // Optional ? may have been consumed as ternary... skip
                let default = if self.eat(&TokenKind::Eq) {
                    Some(self.parse_assign_expr()?)
                } else {
                    None
                };
                params.push(Param { span: expr.span, binding, default, rest: false });
            } else {
                params.push(self.expr_to_param(expr)?);
            }
        }

        // Parse remaining params using parse_binding (with full type support)
        while self.eat(&TokenKind::Comma) {
            if self.check(&TokenKind::RParen) { break; }
            let param_start = self.current.span.start;
            let rest = self.eat(&TokenKind::Spread);
            let binding = self.parse_binding()?;
            self.eat(&TokenKind::Question);
            let default = if self.eat(&TokenKind::Eq) {
                Some(self.parse_assign_expr()?)
            } else {
                None
            };
            let param_end = self.current.span.start;
            params.push(Param { binding, default, rest, span: Span::new(param_start, param_end) });
            if rest { break; }
        }

        self.expect(&TokenKind::RParen)?;
        if self.eat(&TokenKind::Colon) {
            let _ = self.parse_ts_type()?;
        }
        self.expect(&TokenKind::Arrow)?;
        self.parse_arrow_body(params, is_async, start)
    }

    /// Parse arrow function body.
    pub(crate) fn parse_arrow_body(&mut self, params: Vec<Param>, is_async: bool, start: u32) -> Result<Expr, ParseError> {
        let body = if self.check(&TokenKind::LBrace) {
            self.expect(&TokenKind::LBrace)?;
            let mut stmts = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                stmts.push(self.parse_stmt()?);
            }
            self.expect(&TokenKind::RBrace)?;
            ArrowBody::Block(stmts)
        } else {
            ArrowBody::Expr(Box::new(self.parse_assign_expr()?))
        };

        let end = self.current.span.start;
        Ok(Expr::new(
            ExprKind::Arrow(Box::new(ArrowFunction {
                params,
                body,
                is_async,
                span: Span::new(start, end),
                #[cfg(feature = "typescript")]
                type_params: None,
                #[cfg(feature = "typescript")]
                return_type: None,
            })),
            Span::new(start, end),
        ))
    }

    /// Parse template literal.
    fn parse_template_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;
        let mut quasis = Vec::new();
        let mut exprs = Vec::new();

        match self.peek().clone() {
            TokenKind::TemplateNoSub(s) => {
                self.advance();
                return Ok(Expr::new(ExprKind::TemplateNoSub(s), Span::new(start, self.current.span.start)));
            }
            TokenKind::TemplateHead(s) => {
                self.advance();
                quasis.push(s);
            }
            _ => return Err(ParseError::new("Expected template literal", self.current.span)),
        }

        loop {
            exprs.push(Box::new(self.parse_expr()?));

            // After the expression, current token should be `}` closing the ${...}
            if !matches!(self.peek(), TokenKind::RBrace) {
                return Err(ParseError::new("Expected } in template literal", self.current.span));
            }

            // Scan template continuation from the lexer (reads from after `}`)
            let cont_kind = self.lexer.scan_template_continuation();
            self.current = Token::new(cont_kind, self.current.span);

            match self.peek().clone() {
                TokenKind::TemplateMiddle(s) => {
                    self.advance();
                    quasis.push(s);
                }
                TokenKind::TemplateTail(s) => {
                    self.advance();
                    quasis.push(s);
                    break;
                }
                _ => return Err(ParseError::new("Expected template middle or tail", self.current.span)),
            }
        }

        let end = self.current.span.start;
        Ok(Expr::new(ExprKind::Template { quasis, exprs }, Span::new(start, end)))
    }

    // =========================================================================
    // TypeScript Parsing (Feature-gated)
    // =========================================================================

    #[cfg(feature = "typescript")]
    pub(crate) fn parse_ts_type(&mut self) -> Result<TsType, ParseError> {
        self.parse_ts_type_impl()
    }

    #[cfg(feature = "typescript")]
    fn parse_ts_type_alias(&mut self) -> Result<Stmt, ParseError> {
        // Disambiguate: `type Name = ...` is a type alias,
        // but `type = value` or `type.prop` is an expression using `type` as identifier
        let next = self.lexer.peek();
        if matches!(next.kind, TokenKind::Identifier(_)) {
            self.parse_ts_type_alias_impl()
        } else {
            self.parse_expr_stmt()
        }
    }

    #[cfg(feature = "typescript")]
    fn parse_ts_interface(&mut self) -> Result<Stmt, ParseError> {
        self.parse_ts_interface_impl()
    }

    #[cfg(feature = "typescript")]
    fn parse_ts_enum(&mut self) -> Result<Stmt, ParseError> {
        self.parse_ts_enum_impl()
    }

    #[cfg(feature = "typescript")]
    fn parse_ts_namespace(&mut self) -> Result<Stmt, ParseError> {
        self.parse_ts_namespace_impl()
    }

    #[cfg(feature = "typescript")]
    fn parse_ts_declare(&mut self) -> Result<Stmt, ParseError> {
        self.parse_ts_declare_impl()
    }
}

/// Convert a keyword token to its string representation.
pub(crate) fn keyword_to_str(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Var => "var",
        TokenKind::Let => "let",
        TokenKind::Const => "const",
        TokenKind::Function => "function",
        TokenKind::Class => "class",
        TokenKind::If => "if",
        TokenKind::Else => "else",
        TokenKind::Switch => "switch",
        TokenKind::Case => "case",
        TokenKind::Default => "default",
        TokenKind::For => "for",
        TokenKind::While => "while",
        TokenKind::Do => "do",
        TokenKind::Break => "break",
        TokenKind::Continue => "continue",
        TokenKind::Return => "return",
        TokenKind::Try => "try",
        TokenKind::Catch => "catch",
        TokenKind::Finally => "finally",
        TokenKind::Throw => "throw",
        TokenKind::New => "new",
        TokenKind::Delete => "delete",
        TokenKind::Typeof => "typeof",
        TokenKind::Void => "void",
        TokenKind::In => "in",
        TokenKind::Instanceof => "instanceof",
        TokenKind::This => "this",
        TokenKind::Super => "super",
        TokenKind::Null => "null",
        TokenKind::True => "true",
        TokenKind::False => "false",
        TokenKind::Import => "import",
        TokenKind::Export => "export",
        TokenKind::From => "from",
        TokenKind::As => "as",
        TokenKind::Async => "async",
        TokenKind::Await => "await",
        TokenKind::Yield => "yield",
        TokenKind::Static => "static",
        TokenKind::Get => "get",
        TokenKind::Set => "set",
        TokenKind::Extends => "extends",
        TokenKind::With => "with",
        TokenKind::Debugger => "debugger",
        #[cfg(feature = "typescript")]
        TokenKind::Public => "public",
        #[cfg(feature = "typescript")]
        TokenKind::Private => "private",
        #[cfg(feature = "typescript")]
        TokenKind::Protected => "protected",
        #[cfg(feature = "typescript")]
        TokenKind::Implements => "implements",
        #[cfg(feature = "typescript")]
        TokenKind::Abstract => "abstract",
        #[cfg(feature = "typescript")]
        TokenKind::Readonly => "readonly",
        #[cfg(feature = "typescript")]
        TokenKind::Override => "override",
        #[cfg(feature = "typescript")]
        TokenKind::Type => "type",
        #[cfg(feature = "typescript")]
        TokenKind::Interface => "interface",
        #[cfg(feature = "typescript")]
        TokenKind::Enum => "enum",
        #[cfg(feature = "typescript")]
        TokenKind::Namespace => "namespace",
        #[cfg(feature = "typescript")]
        TokenKind::Module => "module",
        #[cfg(feature = "typescript")]
        TokenKind::Declare => "declare",
        #[cfg(feature = "typescript")]
        TokenKind::Keyof => "keyof",
        #[cfg(feature = "typescript")]
        TokenKind::Any => "any",
        #[cfg(feature = "typescript")]
        TokenKind::Unknown => "unknown",
        #[cfg(feature = "typescript")]
        TokenKind::Never => "never",
        #[cfg(feature = "typescript")]
        TokenKind::Is => "is",
        #[cfg(feature = "typescript")]
        TokenKind::Satisfies => "satisfies",
        #[cfg(feature = "typescript")]
        TokenKind::Infer => "infer",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Result<Ast, ParseError> {
        Parser::new(source, ParserOptions::default()).parse()
    }

    #[test]
    fn test_variable_declaration() {
        let ast = parse("let x = 1;").unwrap();
        assert_eq!(ast.stmts.len(), 1);
    }

    #[test]
    fn test_function_declaration() {
        let ast = parse("function foo(a, b) { return a + b; }").unwrap();
        assert_eq!(ast.stmts.len(), 1);
    }

    #[test]
    fn test_binary_expression() {
        let ast = parse("1 + 2 * 3;").unwrap();
        assert_eq!(ast.stmts.len(), 1);
    }

    #[test]
    fn test_arrow_function() {
        let ast = parse("const add = (a, b) => a + b;").unwrap();
        assert_eq!(ast.stmts.len(), 1);
    }

    #[test]
    fn test_class_declaration() {
        let ast = parse("class Foo { constructor() {} bar() {} }").unwrap();
        assert_eq!(ast.stmts.len(), 1);
    }
}
