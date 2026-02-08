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
    fn expect_semicolon(&mut self) -> Result<(), ParseError> {
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
        // 4. After newline (handled by lexer tracking)
        // For now, we just require explicit semicolons or ASI triggers
        // TODO: Track newlines properly for full ASI support
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
    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;

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
            TokenKind::Type => self.parse_ts_type_alias(),
            #[cfg(feature = "typescript")]
            TokenKind::Interface => self.parse_ts_interface(),
            #[cfg(feature = "typescript")]
            TokenKind::Enum => self.parse_ts_enum(),
            #[cfg(feature = "typescript")]
            TokenKind::Namespace | TokenKind::Module => self.parse_ts_namespace(),
            #[cfg(feature = "typescript")]
            TokenKind::Declare => self.parse_ts_declare(),

            // Async function (lookahead required)
            TokenKind::Async => {
                // TODO: Check if followed by function keyword
                self.parse_expr_stmt()
            }

            // Labeled statement or expression statement
            TokenKind::Identifier(_) => {
                // Could be labeled statement: `label: stmt`
                // Or expression statement: `foo();`
                // Need lookahead for colon
                // TODO: Implement lookahead
                self.parse_expr_stmt()
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
    fn parse_var_decl(&mut self) -> Result<Stmt, ParseError> {
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
    fn parse_property_key(&mut self) -> Result<PropertyKey, ParseError> {
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
            _ => Err(ParseError::new(
                format!("Expected property key, got {:?}", self.peek()),
                self.current.span,
            )),
        }
    }

    /// Parse function declaration.
    fn parse_function_decl(&mut self) -> Result<Stmt, ParseError> {
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
        let name = if let TokenKind::Identifier(n) = self.peek() {
            let n = n.clone();
            self.advance();
            Some(n)
        } else {
            None
        };

        // Parameters
        let params = self.parse_params()?;

        // Body
        self.expect(&TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            body.push(self.parse_stmt()?);
        }
        let end = self.current.span.end;
        self.expect(&TokenKind::RBrace)?;

        Ok(Function {
            name,
            params,
            body,
            is_async,
            is_generator,
            span: Span::new(start, end),
            #[cfg(feature = "typescript")]
            type_params: None,
            #[cfg(feature = "typescript")]
            return_type: None,
        })
    }

    /// Parse function parameters.
    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect(&TokenKind::LParen)?;

        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.is_eof() {
            let start = self.current.span.start;
            let rest = self.eat(&TokenKind::Spread);
            let binding = self.parse_binding()?;
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

    /// Parse class declaration.
    fn parse_class_decl(&mut self) -> Result<Stmt, ParseError> {
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

        // Extends clause
        let super_class = if self.eat(&TokenKind::Extends) {
            Some(Box::new(self.parse_left_hand_side_expr()?))
        } else {
            None
        };

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
            type_params: None,
            #[cfg(feature = "typescript")]
            implements: Vec::new(),
        })
    }

    /// Parse a class member.
    fn parse_class_member(&mut self) -> Result<ClassMember, ParseError> {
        let start = self.current.span.start;

        // Check for static
        let is_static = self.eat(&TokenKind::Static);

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

        // Method kind
        let mut method_kind = MethodKind::Method;
        if self.check(&TokenKind::Get) {
            self.advance();
            method_kind = MethodKind::Get;
        } else if self.check(&TokenKind::Set) {
            self.advance();
            method_kind = MethodKind::Set;
        }

        // Property key
        let computed = self.check(&TokenKind::LBracket);
        let key = self.parse_property_key()?;

        // Check for constructor
        if matches!(&key, PropertyKey::Ident(n) if n == "constructor") && !is_static {
            method_kind = MethodKind::Constructor;
        }

        // Method or property?
        if self.check(&TokenKind::LParen) {
            // Method
            let params = self.parse_params()?;
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
                is_async: false,
                is_generator: false,
                span: Span::new(start, end),
                #[cfg(feature = "typescript")]
                type_params: None,
                #[cfg(feature = "typescript")]
                return_type: None,
            };

            Ok(ClassMember {
                kind: ClassMemberKind::Method {
                    key,
                    value: func,
                    kind: method_kind,
                    computed,
                    is_static,
                    #[cfg(feature = "typescript")]
                    accessibility: None,
                    #[cfg(feature = "typescript")]
                    is_abstract: false,
                    #[cfg(feature = "typescript")]
                    is_override: false,
                },
                span: Span::new(start, end),
            })
        } else {
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
                    type_ann: None,
                    #[cfg(feature = "typescript")]
                    accessibility: None,
                    #[cfg(feature = "typescript")]
                    is_readonly: false,
                    #[cfg(feature = "typescript")]
                    is_abstract: false,
                    #[cfg(feature = "typescript")]
                    is_override: false,
                    #[cfg(feature = "typescript")]
                    definite: false,
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
            Some(ForInit::Expr(self.parse_expr()?))
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
        let is_type_only = if let TokenKind::Identifier(id) = self.peek() {
            if id == "type" {
                self.advance();
                true
            } else {
                false
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
                let is_type = if let TokenKind::Identifier(id) = self.peek() {
                    if id == "type" {
                        self.advance();
                        true
                    } else {
                        false
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

    fn parse_export_decl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Export)?;

        #[cfg(feature = "typescript")]
        let is_type_only = if let TokenKind::Identifier(id) = self.peek() {
            if id == "type" {
                self.advance();
                true
            } else {
                false
            }
        } else {
            false
        };

        // export default
        if self.eat(&TokenKind::Default) {
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
                let is_type = if let TokenKind::Identifier(id) = self.peek() {
                    if id == "type" {
                        self.advance();
                        true
                    } else {
                        false
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
            Err(ParseError::new(
                format!("Expected identifier, got {:?}", self.peek()),
                self.current.span,
            ))
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
        self.parse_assign_expr()
    }

    /// Parse an assignment expression.
    pub(crate) fn parse_assign_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;
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
            let op = match self.peek().binary_precedence() {
                Some(prec) if prec >= min_prec => self.get_binary_op().unwrap(),
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
            TokenKind::In => Some(BinaryOp::In),
            TokenKind::Instanceof => Some(BinaryOp::Instanceof),
            _ => None,
        }
    }

    /// Parse unary expression.
    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
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
    fn parse_left_hand_side_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;

        // new expression
        if self.eat(&TokenKind::New) {
            let callee = self.parse_left_hand_side_expr()?;
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
                    let property = self.parse_primary_expr()?;
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
                        let property = self.parse_primary_expr()?;
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
                _ => break,
            }
        }

        Ok(expr)
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
                } else {
                    // Could be async arrow function, for now treat as identifier
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
                // Check for getter/setter
                let mut kind = PropertyKind::Init;
                if self.check(&TokenKind::Get) {
                    self.advance();
                    kind = PropertyKind::Get;
                } else if self.check(&TokenKind::Set) {
                    self.advance();
                    kind = PropertyKind::Set;
                }

                // Property key
                let computed = self.check(&TokenKind::LBracket);
                let key = self.parse_property_key()?;

                // Method shorthand: { foo() {} }
                if self.check(&TokenKind::LParen) {
                    let params = self.parse_params()?;
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
                        is_async: false,
                        is_generator: false,
                        span: Span::new(prop_start, end),
                        #[cfg(feature = "typescript")]
                        type_params: None,
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
        let start = self.current.span.start;
        self.expect(&TokenKind::LParen)?;

        // Empty parens - must be arrow function
        if self.check(&TokenKind::RParen) {
            self.advance();
            self.expect(&TokenKind::Arrow)?;
            return self.parse_arrow_body(Vec::new(), false, start);
        }

        // Parse first element
        let first = self.parse_assign_expr()?;

        // Check for comma (sequence or arrow params)
        if self.eat(&TokenKind::Comma) {
            // Could be sequence expression or arrow function params
            // For now, treat as sequence
            let mut exprs = vec![first];
            while !self.check(&TokenKind::RParen) && !self.is_eof() {
                exprs.push(self.parse_assign_expr()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen)?;

            // Check for arrow
            if self.eat(&TokenKind::Arrow) {
                // Convert expressions to parameters
                let params = self.exprs_to_params(exprs)?;
                return self.parse_arrow_body(params, false, start);
            }

            let end = self.current.span.start;
            return Ok(Expr::new(ExprKind::Sequence(exprs), Span::new(start, end)));
        }

        self.expect(&TokenKind::RParen)?;

        // Check for arrow
        if self.eat(&TokenKind::Arrow) {
            let params = self.exprs_to_params(vec![first])?;
            return self.parse_arrow_body(params, false, start);
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
            _ => Err(ParseError::new(
                "Invalid arrow function parameter",
                expr.span,
            )),
        }
    }

    /// Convert an expression to a binding pattern.
    fn expr_to_binding(&self, expr: Expr) -> Result<Binding, ParseError> {
        match expr.kind {
            ExprKind::Ident(name) => Ok(Binding::new(
                BindingKind::Ident {
                    name,
                    #[cfg(feature = "typescript")]
                    type_ann: None,
                },
                expr.span,
            )),
            // TODO: Handle array and object patterns
            _ => Err(ParseError::new(
                "Invalid binding pattern",
                expr.span,
            )),
        }
    }

    /// Parse arrow function body.
    fn parse_arrow_body(&mut self, params: Vec<Param>, is_async: bool, start: u32) -> Result<Expr, ParseError> {
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
    fn parse_ts_type(&mut self) -> Result<TsType, ParseError> {
        // TODO: Implement TypeScript type parsing
        Err(ParseError::new("TypeScript types not yet implemented", self.current.span))
    }

    #[cfg(feature = "typescript")]
    fn parse_ts_type_alias(&mut self) -> Result<Stmt, ParseError> {
        // TODO: Implement TypeScript type alias parsing
        Err(ParseError::new("TypeScript type alias not yet implemented", self.current.span))
    }

    #[cfg(feature = "typescript")]
    fn parse_ts_interface(&mut self) -> Result<Stmt, ParseError> {
        // TODO: Implement TypeScript interface parsing
        Err(ParseError::new("TypeScript interface not yet implemented", self.current.span))
    }

    #[cfg(feature = "typescript")]
    fn parse_ts_enum(&mut self) -> Result<Stmt, ParseError> {
        // TODO: Implement TypeScript enum parsing
        Err(ParseError::new("TypeScript enum not yet implemented", self.current.span))
    }

    #[cfg(feature = "typescript")]
    fn parse_ts_namespace(&mut self) -> Result<Stmt, ParseError> {
        // TODO: Implement TypeScript namespace parsing
        Err(ParseError::new("TypeScript namespace not yet implemented", self.current.span))
    }

    #[cfg(feature = "typescript")]
    fn parse_ts_declare(&mut self) -> Result<Stmt, ParseError> {
        // TODO: Implement TypeScript declare parsing
        Err(ParseError::new("TypeScript declare not yet implemented", self.current.span))
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
