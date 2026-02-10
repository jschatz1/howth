//! Arena-allocated JavaScript/TypeScript/JSX parser.
//!
//! This parser allocates all AST nodes in a bumpalo arena for ~2-3x speed improvement.

use crate::arena::Arena;
use crate::ast_arena::*;
use crate::lexer::Lexer;
use crate::span::Span;
use crate::token::{Token, TokenKind};
use bumpalo::collections::CollectIn;

/// Parser configuration options.
#[derive(Debug, Clone, Default)]
pub struct ParserOptions {
    pub module: bool,
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

/// Arena-based parser for maximum speed.
pub struct ArenaParser<'a> {
    arena: &'a Arena,
    lexer: Lexer<'a>,
    current: Token,
    source: &'a str,
}

impl<'a> ArenaParser<'a> {
    /// Create a new arena parser.
    pub fn new(arena: &'a Arena, source: &'a str, _options: ParserOptions) -> Self {
        let mut lexer = Lexer::new(source);
        let current = lexer.next_token();
        Self {
            arena,
            lexer,
            current,
            source,
        }
    }

    /// Parse the program.
    pub fn parse(mut self) -> Result<Program<'a>, ParseError> {
        let start = self.current.span.start;
        let mut stmts = self.arena.vec();

        while !self.is_eof() {
            stmts.push(self.parse_statement()?);
        }

        let end = self.current.span.end;
        let stmts_slice = stmts
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice();
        Ok(Program::new(stmts_slice, Span::new(start, end)))
    }

    // =========================================================================
    // Token Handling
    // =========================================================================

    fn peek(&self) -> &TokenKind {
        &self.current.kind
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    fn is_identifier(&self) -> bool {
        matches!(self.peek(), TokenKind::Identifier(_))
    }

    fn advance(&mut self) -> Token {
        std::mem::replace(&mut self.current, self.lexer.next_token())
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(&kind) {
            Ok(self.advance())
        } else {
            Err(ParseError::new(
                format!("Expected {:?}, got {:?}", kind, self.peek()),
                self.current.span,
            ))
        }
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(&kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn span_text(&self, span: Span) -> &str {
        &self.source[span.start as usize..span.end as usize]
    }

    // =========================================================================
    // Statement Parsing
    // =========================================================================

    fn parse_statement(&mut self) -> Result<Stmt<'a>, ParseError> {
        let start = self.current.span.start;

        let kind = match self.peek() {
            TokenKind::Let | TokenKind::Const | TokenKind::Var => self.parse_var_decl()?,
            TokenKind::Function => self.parse_function_decl()?,
            TokenKind::Class => self.parse_class_decl()?,
            TokenKind::Return => self.parse_return()?,
            TokenKind::If => self.parse_if()?,
            TokenKind::Switch => self.parse_switch()?,
            TokenKind::While => self.parse_while()?,
            TokenKind::Do => self.parse_do_while()?,
            TokenKind::For => self.parse_for()?,
            TokenKind::Break => self.parse_break()?,
            TokenKind::Continue => self.parse_continue()?,
            TokenKind::Throw => self.parse_throw()?,
            TokenKind::Try => self.parse_try()?,
            TokenKind::LBrace => self.parse_block()?,
            TokenKind::Import => self.parse_import()?,
            TokenKind::Export => self.parse_export()?,
            TokenKind::Debugger => {
                self.advance();
                self.eat(TokenKind::Semicolon);
                StmtKind::Debugger
            }
            TokenKind::Semicolon => {
                self.advance();
                StmtKind::Empty
            }
            _ => {
                let expr = self.parse_expression()?;
                self.eat(TokenKind::Semicolon);
                StmtKind::Expr(expr)
            }
        };

        let end = self.current.span.end;
        Ok(Stmt::new(kind, Span::new(start, end)))
    }

    fn parse_var_decl(&mut self) -> Result<StmtKind<'a>, ParseError> {
        let var_kind = match self.peek() {
            TokenKind::Let => {
                self.advance();
                VarKind::Let
            }
            TokenKind::Const => {
                self.advance();
                VarKind::Const
            }
            TokenKind::Var => {
                self.advance();
                VarKind::Var
            }
            _ => return Err(ParseError::new("Expected var/let/const", self.current.span)),
        };

        let mut decls = self.arena.vec();

        loop {
            let start = self.current.span.start;
            let binding = self.parse_binding()?;
            let init = if self.eat(TokenKind::Eq) {
                Some(self.parse_expression()?)
            } else {
                None
            };
            let end = self.current.span.end;
            decls.push(VarDeclarator {
                binding,
                init,
                span: Span::new(start, end),
            });

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.eat(TokenKind::Semicolon);
        let decls_slice = decls
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice();
        Ok(StmtKind::Var {
            kind: var_kind,
            decls: decls_slice,
        })
    }

    fn parse_binding(&mut self) -> Result<Binding<'a>, ParseError> {
        let start = self.current.span.start;

        let kind = match self.peek() {
            TokenKind::Identifier(_) => {
                let name = self.parse_ident()?;
                BindingKind::Ident { name }
            }
            TokenKind::LBracket => {
                return self.parse_array_binding();
            }
            TokenKind::LBrace => {
                return self.parse_object_binding();
            }
            _ => {
                return Err(ParseError::new(
                    format!("Expected identifier, '[', or '{{', got {:?}", self.peek()),
                    self.current.span,
                ));
            }
        };

        let end = self.current.span.end;
        Ok(Binding::new(kind, Span::new(start, end)))
    }

    fn parse_array_binding(&mut self) -> Result<Binding<'a>, ParseError> {
        let start = self.current.span.start;
        self.expect(TokenKind::LBracket)?;

        let mut elements = self.arena.vec();
        while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
            if self.eat(TokenKind::Comma) {
                // Elision
                elements.push(None);
            } else {
                let rest = self.eat(TokenKind::Spread);
                let binding = self.parse_binding()?;
                let default = if self.eat(TokenKind::Eq) {
                    Some(self.parse_assignment()?)
                } else {
                    None
                };
                elements.push(Some(ArrayPatternElement {
                    binding,
                    default,
                    rest,
                }));

                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(TokenKind::RBracket)?;
        let end = self.current.span.end;

        let elements_slice = elements
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice();
        Ok(Binding::new(
            BindingKind::Array {
                elements: elements_slice,
            },
            Span::new(start, end),
        ))
    }

    fn parse_object_binding(&mut self) -> Result<Binding<'a>, ParseError> {
        let start = self.current.span.start;
        self.expect(TokenKind::LBrace)?;

        let mut properties = self.arena.vec();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let rest = self.eat(TokenKind::Spread);

            if rest {
                // Rest element: `...rest`
                let binding = self.parse_binding()?;
                properties.push(ObjectPatternProperty {
                    key: PropertyKey::Ident(self.arena.alloc_str("")),
                    value: binding,
                    default: None,
                    shorthand: false,
                    rest: true,
                });
            } else {
                // Property: `key` or `key: value` or `key = default`
                let key = self.parse_property_key()?;

                if self.eat(TokenKind::Colon) {
                    // `key: value`
                    let value = self.parse_binding()?;
                    let default = if self.eat(TokenKind::Eq) {
                        Some(self.parse_assignment()?)
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
                    let name = match key {
                        PropertyKey::Ident(n) => n,
                        _ => {
                            return Err(ParseError::new(
                                "Expected identifier in shorthand property",
                                self.current.span,
                            ))
                        }
                    };
                    let default = if self.eat(TokenKind::Eq) {
                        Some(self.parse_assignment()?)
                    } else {
                        None
                    };
                    let value_span = self.current.span;
                    properties.push(ObjectPatternProperty {
                        key: PropertyKey::Ident(name),
                        value: Binding::new(BindingKind::Ident { name }, value_span),
                        default,
                        shorthand: true,
                        rest: false,
                    });
                }
            }

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::RBrace)?;
        let end = self.current.span.end;

        let props_slice = properties
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice();
        Ok(Binding::new(
            BindingKind::Object {
                properties: props_slice,
            },
            Span::new(start, end),
        ))
    }

    fn parse_ident(&mut self) -> Result<&'a str, ParseError> {
        if let TokenKind::Identifier(_) = self.peek() {
            let token = self.advance();
            let name = self.span_text(token.span);
            Ok(self.arena.alloc_str(name))
        } else {
            Err(ParseError::new("Expected identifier", self.current.span))
        }
    }

    fn parse_function_decl(&mut self) -> Result<StmtKind<'a>, ParseError> {
        let func = self.parse_function()?;
        let func_ref = self.arena.alloc(func);
        Ok(StmtKind::Function(func_ref))
    }

    fn parse_function(&mut self) -> Result<Function<'a>, ParseError> {
        let start = self.current.span.start;

        let is_async = self.eat(TokenKind::Async);
        self.expect(TokenKind::Function)?;
        let is_generator = self.eat(TokenKind::Star);

        let name = if self.is_identifier() {
            Some(self.parse_ident()?)
        } else {
            None
        };

        self.expect(TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(TokenKind::RParen)?;

        self.expect(TokenKind::LBrace)?;
        let body = self.parse_block_body()?;
        self.expect(TokenKind::RBrace)?;

        let end = self.current.span.end;
        Ok(Function {
            name,
            params,
            body,
            is_async,
            is_generator,
            span: Span::new(start, end),
        })
    }

    fn parse_params(&mut self) -> Result<&'a [Param<'a>], ParseError> {
        let mut params = self.arena.vec();

        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            let start = self.current.span.start;
            let rest = self.eat(TokenKind::Spread);
            let binding = self.parse_binding()?;
            let default = if self.eat(TokenKind::Eq) {
                Some(self.parse_expression()?)
            } else {
                None
            };
            let end = self.current.span.end;
            params.push(Param {
                binding,
                default,
                rest,
                span: Span::new(start, end),
            });

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        Ok(params
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice())
    }

    fn parse_block_body(&mut self) -> Result<&'a [Stmt<'a>], ParseError> {
        let mut stmts = self.arena.vec();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            stmts.push(self.parse_statement()?);
        }
        Ok(stmts
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice())
    }

    fn parse_class_decl(&mut self) -> Result<StmtKind<'a>, ParseError> {
        let class = self.parse_class()?;
        let class_ref = self.arena.alloc(class);
        Ok(StmtKind::Class(class_ref))
    }

    fn parse_class(&mut self) -> Result<Class<'a>, ParseError> {
        let start = self.current.span.start;
        self.expect(TokenKind::Class)?;

        let name = if self.is_identifier() {
            Some(self.parse_ident()?)
        } else {
            None
        };

        let super_class = if self.eat(TokenKind::Extends) {
            let expr = self.parse_expression()?;
            Some(self.arena.alloc(expr))
        } else {
            None
        };

        self.expect(TokenKind::LBrace)?;
        let mut members = self.arena.vec();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            members.push(self.parse_class_member()?);
        }
        self.expect(TokenKind::RBrace)?;

        let end = self.current.span.end;
        Ok(Class {
            name,
            super_class,
            body: members
                .into_iter()
                .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
                .into_bump_slice(),
            span: Span::new(start, end),
        })
    }

    fn parse_class_member(&mut self) -> Result<ClassMember<'a>, ParseError> {
        let start = self.current.span.start;
        let is_static = self.eat(TokenKind::Static);

        // Skip static blocks for now
        if is_static && matches!(self.peek(), TokenKind::LBrace) {
            self.advance();
            let body = self.parse_block_body()?;
            self.expect(TokenKind::RBrace)?;
            return Ok(ClassMember {
                kind: ClassMemberKind::StaticBlock(body),
                span: Span::new(start, self.current.span.end),
            });
        }

        let key = self.parse_property_key()?;

        if matches!(self.peek(), TokenKind::LParen) {
            // Method
            self.expect(TokenKind::LParen)?;
            let params = self.parse_params()?;
            self.expect(TokenKind::RParen)?;
            self.expect(TokenKind::LBrace)?;
            let body = self.parse_block_body()?;
            self.expect(TokenKind::RBrace)?;

            let end = self.current.span.end;
            let func = Function {
                name: None,
                params,
                body,
                is_async: false,
                is_generator: false,
                span: Span::new(start, end),
            };

            Ok(ClassMember {
                kind: ClassMemberKind::Method {
                    key,
                    value: self.arena.alloc(func),
                    kind: MethodKind::Method,
                    computed: false,
                    is_static,
                },
                span: Span::new(start, end),
            })
        } else {
            // Property
            let value = if self.eat(TokenKind::Eq) {
                Some(self.parse_expression()?)
            } else {
                None
            };
            self.eat(TokenKind::Semicolon);

            let end = self.current.span.end;
            Ok(ClassMember {
                kind: ClassMemberKind::Property {
                    key,
                    value,
                    computed: false,
                    is_static,
                },
                span: Span::new(start, end),
            })
        }
    }

    fn parse_property_key(&mut self) -> Result<PropertyKey<'a>, ParseError> {
        match self.peek() {
            TokenKind::Identifier(_) => {
                let name = self.parse_ident()?;
                Ok(PropertyKey::Ident(name))
            }
            TokenKind::String(_) => {
                let token = self.advance();
                let s = self.span_text(token.span);
                let s = &s[1..s.len() - 1]; // Remove quotes
                Ok(PropertyKey::String(self.arena.alloc_str(s)))
            }
            TokenKind::Number(_) => {
                let token = self.advance();
                let s = self.span_text(token.span);
                let n: f64 = s.parse().unwrap_or(0.0);
                Ok(PropertyKey::Number(n))
            }
            _ => Err(ParseError::new("Expected property key", self.current.span)),
        }
    }

    fn parse_return(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::Return)?;
        let arg = if !matches!(
            self.peek(),
            TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof
        ) {
            Some(self.parse_expression()?)
        } else {
            None
        };
        self.eat(TokenKind::Semicolon);
        Ok(StmtKind::Return { arg })
    }

    fn parse_if(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::If)?;
        self.expect(TokenKind::LParen)?;
        let test = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;

        let consequent = self.parse_statement()?;
        let consequent = self.arena.alloc(consequent);

        let alternate = if self.eat(TokenKind::Else) {
            Some(self.arena.alloc(self.parse_statement()?))
        } else {
            None
        };

        Ok(StmtKind::If {
            test,
            consequent,
            alternate,
        })
    }

    fn parse_while(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::While)?;
        self.expect(TokenKind::LParen)?;
        let test = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        let body = self.arena.alloc(self.parse_statement()?);
        Ok(StmtKind::While { test, body })
    }

    fn parse_for(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::For)?;

        // Check for await in for-await-of
        let is_await = self.eat(TokenKind::Await);

        self.expect(TokenKind::LParen)?;

        // Check for semicolon (empty init in regular for)
        if self.eat(TokenKind::Semicolon) {
            return self.parse_regular_for(None);
        }

        // Parse the left-hand side (could be var decl or expression)
        if matches!(
            self.peek(),
            TokenKind::Var | TokenKind::Let | TokenKind::Const
        ) {
            let var_kind = match self.peek() {
                TokenKind::Var => {
                    self.advance();
                    VarKind::Var
                }
                TokenKind::Let => {
                    self.advance();
                    VarKind::Let
                }
                TokenKind::Const => {
                    self.advance();
                    VarKind::Const
                }
                _ => unreachable!(),
            };
            let start = self.current.span.start;
            let binding = self.parse_binding()?;

            // Check for in/of
            if self.eat(TokenKind::In) {
                let left = ForInit::Var {
                    kind: var_kind,
                    decls: self
                        .arena
                        .alloc_slice_from_iter(std::iter::once(VarDeclarator {
                            binding,
                            init: None,
                            span: Span::new(start, self.current.span.end),
                        })),
                };
                let right = self.parse_expression()?;
                self.expect(TokenKind::RParen)?;
                let body = self.arena.alloc(self.parse_statement()?);
                return Ok(StmtKind::ForIn { left, right, body });
            }

            if self.is_contextual_keyword("of") {
                self.advance(); // consume 'of'
                let left = ForInit::Var {
                    kind: var_kind,
                    decls: self
                        .arena
                        .alloc_slice_from_iter(std::iter::once(VarDeclarator {
                            binding,
                            init: None,
                            span: Span::new(start, self.current.span.end),
                        })),
                };
                let right = self.parse_assignment()?;
                self.expect(TokenKind::RParen)?;
                let body = self.arena.alloc(self.parse_statement()?);
                return Ok(StmtKind::ForOf {
                    left,
                    right,
                    body,
                    is_await,
                });
            }

            // Regular for loop with var decl
            let init_val = if self.eat(TokenKind::Eq) {
                Some(self.parse_expression()?)
            } else {
                None
            };
            let end = self.current.span.end;
            let decl = VarDeclarator {
                binding,
                init: init_val,
                span: Span::new(start, end),
            };
            self.expect(TokenKind::Semicolon)?;
            let slice = self.arena.alloc_slice_from_iter(std::iter::once(decl));
            return self.parse_regular_for(Some(ForInit::Var {
                kind: var_kind,
                decls: slice,
            }));
        }

        // Expression in for init
        let expr = self.parse_expression()?;

        // Check for in/of with expression
        if self.eat(TokenKind::In) {
            let left = ForInit::Expr(expr);
            let right = self.parse_expression()?;
            self.expect(TokenKind::RParen)?;
            let body = self.arena.alloc(self.parse_statement()?);
            return Ok(StmtKind::ForIn { left, right, body });
        }

        if self.is_contextual_keyword("of") {
            self.advance(); // consume 'of'
            let left = ForInit::Expr(expr);
            let right = self.parse_assignment()?;
            self.expect(TokenKind::RParen)?;
            let body = self.arena.alloc(self.parse_statement()?);
            return Ok(StmtKind::ForOf {
                left,
                right,
                body,
                is_await,
            });
        }

        // Regular for loop
        self.expect(TokenKind::Semicolon)?;
        self.parse_regular_for(Some(ForInit::Expr(expr)))
    }

    fn parse_regular_for(&mut self, init: Option<ForInit<'a>>) -> Result<StmtKind<'a>, ParseError> {
        let test = if self.eat(TokenKind::Semicolon) {
            None
        } else {
            let expr = self.parse_expression()?;
            self.expect(TokenKind::Semicolon)?;
            Some(expr)
        };

        let update = if matches!(self.peek(), TokenKind::RParen) {
            None
        } else {
            Some(self.parse_expression()?)
        };

        self.expect(TokenKind::RParen)?;
        let body = self.arena.alloc(self.parse_statement()?);

        Ok(StmtKind::For {
            init,
            test,
            update,
            body,
        })
    }

    fn is_contextual_keyword(&self, keyword: &str) -> bool {
        if let TokenKind::Identifier(name) = self.peek() {
            name == keyword
        } else {
            false
        }
    }

    fn parse_block(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::LBrace)?;
        let body = self.parse_block_body()?;
        self.expect(TokenKind::RBrace)?;
        Ok(StmtKind::Block(body))
    }

    fn parse_switch(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::Switch)?;
        self.expect(TokenKind::LParen)?;
        let discriminant = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;

        let mut cases = self.arena.vec();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let case_start = self.current.span.start;

            let test = if self.eat(TokenKind::Case) {
                Some(self.parse_expression()?)
            } else if self.eat(TokenKind::Default) {
                None
            } else {
                return Err(ParseError::new(
                    "Expected 'case' or 'default'",
                    self.current.span,
                ));
            };

            self.expect(TokenKind::Colon)?;

            let mut consequent = self.arena.vec();
            while !matches!(
                self.peek(),
                TokenKind::Case | TokenKind::Default | TokenKind::RBrace | TokenKind::Eof
            ) {
                consequent.push(self.parse_statement()?);
            }

            let case_end = self.current.span.end;
            cases.push(SwitchCase {
                test,
                consequent: consequent
                    .into_iter()
                    .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
                    .into_bump_slice(),
                span: Span::new(case_start, case_end),
            });
        }

        self.expect(TokenKind::RBrace)?;
        let cases_slice = cases
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice();
        Ok(StmtKind::Switch {
            discriminant,
            cases: cases_slice,
        })
    }

    fn parse_do_while(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::Do)?;
        let body = self.arena.alloc(self.parse_statement()?);
        self.expect(TokenKind::While)?;
        self.expect(TokenKind::LParen)?;
        let test = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        self.eat(TokenKind::Semicolon);
        Ok(StmtKind::DoWhile { body, test })
    }

    fn parse_break(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::Break)?;
        let label = if self.is_identifier() && !self.has_line_terminator_before() {
            Some(self.parse_ident()?)
        } else {
            None
        };
        self.eat(TokenKind::Semicolon);
        Ok(StmtKind::Break { label })
    }

    fn parse_continue(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::Continue)?;
        let label = if self.is_identifier() && !self.has_line_terminator_before() {
            Some(self.parse_ident()?)
        } else {
            None
        };
        self.eat(TokenKind::Semicolon);
        Ok(StmtKind::Continue { label })
    }

    fn parse_throw(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::Throw)?;
        let arg = self.parse_expression()?;
        self.eat(TokenKind::Semicolon);
        Ok(StmtKind::Throw { arg })
    }

    fn parse_try(&mut self) -> Result<StmtKind<'a>, ParseError> {
        self.expect(TokenKind::Try)?;
        self.expect(TokenKind::LBrace)?;
        let block = self.parse_block_body()?;
        self.expect(TokenKind::RBrace)?;

        let handler = if self.eat(TokenKind::Catch) {
            let catch_start = self.current.span.start;
            let param = if self.eat(TokenKind::LParen) {
                let binding = self.parse_binding()?;
                self.expect(TokenKind::RParen)?;
                Some(binding)
            } else {
                None
            };
            self.expect(TokenKind::LBrace)?;
            let body = self.parse_block_body()?;
            self.expect(TokenKind::RBrace)?;
            let catch_end = self.current.span.end;
            Some(CatchClause {
                param,
                body,
                span: Span::new(catch_start, catch_end),
            })
        } else {
            None
        };

        let finalizer = if self.eat(TokenKind::Finally) {
            self.expect(TokenKind::LBrace)?;
            let body = self.parse_block_body()?;
            self.expect(TokenKind::RBrace)?;
            Some(body)
        } else {
            None
        };

        Ok(StmtKind::Try {
            block,
            handler,
            finalizer,
        })
    }

    /// Check if there was a line terminator before the current token.
    /// This is a simplified version - for proper ASI we'd need lexer support.
    fn has_line_terminator_before(&self) -> bool {
        // For now, just return false - proper implementation would check lexer
        false
    }

    // =========================================================================
    // Import/Export Parsing
    // =========================================================================

    fn parse_import(&mut self) -> Result<StmtKind<'a>, ParseError> {
        let start = self.current.span.start;
        self.expect(TokenKind::Import)?;

        let mut specifiers = self.arena.vec();

        // Side-effect import: import 'module';
        if let TokenKind::String(_) = self.peek() {
            let source = self.parse_string_literal()?;
            self.eat(TokenKind::Semicolon);
            let end = self.current.span.end;
            let import = ImportDecl {
                specifiers: &[],
                source,
                span: Span::new(start, end),
            };
            return Ok(StmtKind::Import(self.arena.alloc(import)));
        }

        // Default import: import foo from 'module';
        if self.is_identifier() {
            let spec_start = self.current.span.start;
            let local = self.parse_ident()?;
            let spec_end = self.current.span.end;
            specifiers.push(ImportSpecifier::Default {
                local,
                span: Span::new(spec_start, spec_end),
            });

            if self.eat(TokenKind::Comma) {
                // Continue to namespace or named imports
            } else {
                // Just default import
                self.expect(TokenKind::From)?;
                let source = self.parse_string_literal()?;
                self.eat(TokenKind::Semicolon);
                let end = self.current.span.end;
                let specs = specifiers
                    .into_iter()
                    .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
                    .into_bump_slice();
                let import = ImportDecl {
                    specifiers: specs,
                    source,
                    span: Span::new(start, end),
                };
                return Ok(StmtKind::Import(self.arena.alloc(import)));
            }
        }

        // Namespace import: import * as name from 'module';
        if self.eat(TokenKind::Star) {
            self.expect(TokenKind::As)?;
            let spec_start = self.current.span.start;
            let local = self.parse_ident()?;
            let spec_end = self.current.span.end;
            specifiers.push(ImportSpecifier::Namespace {
                local,
                span: Span::new(spec_start, spec_end),
            });
        }

        // Named imports: import { a, b as c } from 'module';
        if self.eat(TokenKind::LBrace) {
            while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                let spec_start = self.current.span.start;
                let imported = self.parse_ident()?;
                let local = if self.eat(TokenKind::As) {
                    self.parse_ident()?
                } else {
                    imported
                };
                let spec_end = self.current.span.end;
                specifiers.push(ImportSpecifier::Named {
                    imported,
                    local,
                    span: Span::new(spec_start, spec_end),
                });

                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RBrace)?;
        }

        self.expect(TokenKind::From)?;
        let source = self.parse_string_literal()?;
        self.eat(TokenKind::Semicolon);
        let end = self.current.span.end;

        let specs = specifiers
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice();
        let import = ImportDecl {
            specifiers: specs,
            source,
            span: Span::new(start, end),
        };
        Ok(StmtKind::Import(self.arena.alloc(import)))
    }

    fn parse_export(&mut self) -> Result<StmtKind<'a>, ParseError> {
        let start = self.current.span.start;
        self.expect(TokenKind::Export)?;

        // export default expr;
        if self.eat(TokenKind::Default) {
            let expr = self.parse_expression()?;
            self.eat(TokenKind::Semicolon);
            let end = self.current.span.end;
            let export = ExportDecl::Default {
                expr,
                span: Span::new(start, end),
            };
            return Ok(StmtKind::Export(self.arena.alloc(export)));
        }

        // export * from 'module';
        // export * as name from 'module';
        if self.eat(TokenKind::Star) {
            let exported = if self.eat(TokenKind::As) {
                Some(self.parse_ident()?)
            } else {
                None
            };
            self.expect(TokenKind::From)?;
            let source = self.parse_string_literal()?;
            self.eat(TokenKind::Semicolon);
            let end = self.current.span.end;
            let export = ExportDecl::All {
                exported,
                source,
                span: Span::new(start, end),
            };
            return Ok(StmtKind::Export(self.arena.alloc(export)));
        }

        // export { a, b as c };
        // export { a, b } from 'module';
        if self.eat(TokenKind::LBrace) {
            let mut specifiers = self.arena.vec();
            while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                let spec_start = self.current.span.start;
                let local = self.parse_ident()?;
                let exported = if self.eat(TokenKind::As) {
                    self.parse_ident()?
                } else {
                    local
                };
                let spec_end = self.current.span.end;
                specifiers.push(ExportSpecifier {
                    local,
                    exported,
                    span: Span::new(spec_start, spec_end),
                });

                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RBrace)?;

            let source = if self.eat(TokenKind::From) {
                Some(self.parse_string_literal()?)
            } else {
                None
            };

            self.eat(TokenKind::Semicolon);
            let end = self.current.span.end;

            let specs = specifiers
                .into_iter()
                .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
                .into_bump_slice();
            let export = ExportDecl::Named {
                specifiers: specs,
                source,
                span: Span::new(start, end),
            };
            return Ok(StmtKind::Export(self.arena.alloc(export)));
        }

        // export function/class/const/let/var declaration
        let decl = self.parse_statement()?;
        let end = self.current.span.end;
        let export = ExportDecl::Decl {
            decl: self.arena.alloc(decl),
            span: Span::new(start, end),
        };
        Ok(StmtKind::Export(self.arena.alloc(export)))
    }

    fn parse_string_literal(&mut self) -> Result<&'a str, ParseError> {
        if let TokenKind::String(_) = self.peek() {
            let token = self.advance();
            let s = self.span_text(token.span);
            // Remove quotes
            let s = &s[1..s.len() - 1];
            Ok(self.arena.alloc_str(s))
        } else {
            Err(ParseError::new(
                "Expected string literal",
                self.current.span,
            ))
        }
    }

    // =========================================================================
    // Expression Parsing (Pratt parser)
    // =========================================================================

    fn parse_expression(&mut self) -> Result<Expr<'a>, ParseError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr<'a>, ParseError> {
        let left = self.parse_conditional()?;

        if let Some(op) = self.assignment_op() {
            self.advance();
            let right = self.parse_assignment()?;
            let span = Span::new(left.span.start, right.span.end);
            let left = self.arena.alloc(left);
            let right = self.arena.alloc(right);
            return Ok(Expr::new(ExprKind::Assign { op, left, right }, span));
        }

        Ok(left)
    }

    fn assignment_op(&self) -> Option<AssignOp> {
        match self.peek() {
            TokenKind::Eq => Some(AssignOp::Assign),
            TokenKind::PlusEq => Some(AssignOp::AddAssign),
            TokenKind::MinusEq => Some(AssignOp::SubAssign),
            TokenKind::StarEq => Some(AssignOp::MulAssign),
            TokenKind::SlashEq => Some(AssignOp::DivAssign),
            TokenKind::PercentEq => Some(AssignOp::ModAssign),
            _ => None,
        }
    }

    fn parse_conditional(&mut self) -> Result<Expr<'a>, ParseError> {
        let test = self.parse_binary(0)?;

        if self.eat(TokenKind::Question) {
            let consequent = self.parse_assignment()?;
            self.expect(TokenKind::Colon)?;
            let alternate = self.parse_assignment()?;
            let span = Span::new(test.span.start, alternate.span.end);
            return Ok(Expr::new(
                ExprKind::Conditional {
                    test: self.arena.alloc(test),
                    consequent: self.arena.alloc(consequent),
                    alternate: self.arena.alloc(alternate),
                },
                span,
            ));
        }

        Ok(test)
    }

    fn parse_binary(&mut self, min_prec: u8) -> Result<Expr<'a>, ParseError> {
        let mut left = self.parse_unary()?;

        while let Some((op, prec)) = self.binary_op() {
            if prec < min_prec {
                break;
            }

            self.advance();
            let right = self.parse_binary(prec + 1)?;
            let span = Span::new(left.span.start, right.span.end);
            left = Expr::new(
                ExprKind::Binary {
                    op,
                    left: self.arena.alloc(left),
                    right: self.arena.alloc(right),
                },
                span,
            );
        }

        Ok(left)
    }

    fn binary_op(&self) -> Option<(BinaryOp, u8)> {
        match self.peek() {
            TokenKind::PipePipe => Some((BinaryOp::Or, 1)),
            TokenKind::AmpAmp => Some((BinaryOp::And, 2)),
            TokenKind::Pipe => Some((BinaryOp::BitOr, 3)),
            TokenKind::Caret => Some((BinaryOp::BitXor, 4)),
            TokenKind::Amp => Some((BinaryOp::BitAnd, 5)),
            TokenKind::EqEq => Some((BinaryOp::Eq, 6)),
            TokenKind::BangEq => Some((BinaryOp::NotEq, 6)),
            TokenKind::EqEqEq => Some((BinaryOp::StrictEq, 6)),
            TokenKind::BangEqEq => Some((BinaryOp::StrictNotEq, 6)),
            TokenKind::Lt => Some((BinaryOp::Lt, 7)),
            TokenKind::LtEq => Some((BinaryOp::LtEq, 7)),
            TokenKind::Gt => Some((BinaryOp::Gt, 7)),
            TokenKind::GtEq => Some((BinaryOp::GtEq, 7)),
            TokenKind::Plus => Some((BinaryOp::Add, 9)),
            TokenKind::Minus => Some((BinaryOp::Sub, 9)),
            TokenKind::Star => Some((BinaryOp::Mul, 10)),
            TokenKind::Slash => Some((BinaryOp::Div, 10)),
            TokenKind::Percent => Some((BinaryOp::Mod, 10)),
            TokenKind::StarStar => Some((BinaryOp::Pow, 11)),
            _ => None,
        }
    }

    fn parse_unary(&mut self) -> Result<Expr<'a>, ParseError> {
        let start = self.current.span.start;

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
            let arg = self.parse_unary()?;
            let span = Span::new(start, arg.span.end);
            return Ok(Expr::new(
                ExprKind::Unary {
                    op,
                    arg: self.arena.alloc(arg),
                },
                span,
            ));
        }

        // Update prefix: ++x, --x
        if matches!(self.peek(), TokenKind::PlusPlus | TokenKind::MinusMinus) {
            let op = if matches!(self.peek(), TokenKind::PlusPlus) {
                UpdateOp::Increment
            } else {
                UpdateOp::Decrement
            };
            self.advance();
            let arg = self.parse_unary()?;
            let span = Span::new(start, arg.span.end);
            return Ok(Expr::new(
                ExprKind::Update {
                    op,
                    prefix: true,
                    arg: self.arena.alloc(arg),
                },
                span,
            ));
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr<'a>, ParseError> {
        let mut expr = self.parse_call()?;

        // Update postfix: x++, x--
        if matches!(self.peek(), TokenKind::PlusPlus | TokenKind::MinusMinus) {
            let op = if matches!(self.peek(), TokenKind::PlusPlus) {
                UpdateOp::Increment
            } else {
                UpdateOp::Decrement
            };
            let end = self.advance().span.end;
            let span = Span::new(expr.span.start, end);
            expr = Expr::new(
                ExprKind::Update {
                    op,
                    prefix: false,
                    arg: self.arena.alloc(expr),
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_call(&mut self) -> Result<Expr<'a>, ParseError> {
        let mut expr = self.parse_member()?;

        while let TokenKind::LParen = self.peek() {
            self.advance();
            let args = self.parse_arguments()?;
            let end = self.expect(TokenKind::RParen)?.span.end;
            let span = Span::new(expr.span.start, end);
            expr = Expr::new(
                ExprKind::Call {
                    callee: self.arena.alloc(expr),
                    args,
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_arguments(&mut self) -> Result<&'a [Expr<'a>], ParseError> {
        let mut args = self.arena.vec();

        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            if self.eat(TokenKind::Spread) {
                // Spread argument: ...expr
                let start = self.current.span.start;
                let arg = self.parse_assignment()?;
                let span = Span::new(start, arg.span.end);
                args.push(Expr::new(ExprKind::Spread(self.arena.alloc(arg)), span));
            } else {
                args.push(self.parse_assignment()?);
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        Ok(args
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice())
    }

    fn parse_member(&mut self) -> Result<Expr<'a>, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek() {
                TokenKind::Dot => {
                    self.advance();
                    let prop_name = self.parse_ident()?;
                    let prop_span = self.current.span;
                    let property = Expr::new(ExprKind::Ident(prop_name), prop_span);
                    let span = Span::new(expr.span.start, prop_span.end);
                    expr = Expr::new(
                        ExprKind::Member {
                            object: self.arena.alloc(expr),
                            property: self.arena.alloc(property),
                            computed: false,
                        },
                        span,
                    );
                }
                TokenKind::LBracket => {
                    self.advance();
                    let property = self.parse_expression()?;
                    let end = self.expect(TokenKind::RBracket)?.span.end;
                    let span = Span::new(expr.span.start, end);
                    expr = Expr::new(
                        ExprKind::Member {
                            object: self.arena.alloc(expr),
                            property: self.arena.alloc(property),
                            computed: true,
                        },
                        span,
                    );
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr<'a>, ParseError> {
        let start = self.current.span.start;

        let kind = match self.peek() {
            TokenKind::Null => {
                self.advance();
                ExprKind::Null
            }
            TokenKind::True => {
                self.advance();
                ExprKind::Bool(true)
            }
            TokenKind::False => {
                self.advance();
                ExprKind::Bool(false)
            }
            TokenKind::This => {
                self.advance();
                ExprKind::This
            }
            TokenKind::New => {
                return self.parse_new_expr();
            }
            TokenKind::Await => {
                self.advance();
                let arg = self.parse_unary()?;
                let span = Span::new(start, arg.span.end);
                return Ok(Expr::new(ExprKind::Await(self.arena.alloc(arg)), span));
            }
            TokenKind::Number(_) => {
                let token = self.advance();
                let s = self.span_text(token.span);
                ExprKind::Number(s.parse().unwrap_or(0.0))
            }
            TokenKind::String(_) => {
                let token = self.advance();
                let s = self.span_text(token.span);
                let s = &s[1..s.len() - 1]; // Remove quotes
                ExprKind::String(self.arena.alloc_str(s))
            }
            TokenKind::Identifier(_) => {
                // Check for arrow function: ident => ...
                if self.is_arrow_function_start() {
                    return self.parse_arrow_function();
                }
                let name = self.parse_ident()?;
                ExprKind::Ident(name)
            }
            TokenKind::LParen => {
                // Could be arrow function or parenthesized expression
                if self.is_arrow_function_start() {
                    return self.parse_arrow_function();
                }
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(TokenKind::RParen)?;
                ExprKind::Paren(self.arena.alloc(expr))
            }
            TokenKind::LBracket => {
                self.advance();
                let mut elements = self.arena.vec();
                while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                    if self.eat(TokenKind::Comma) {
                        elements.push(None);
                    } else if self.eat(TokenKind::Spread) {
                        // Spread element: ...expr
                        let start = self.current.span.start;
                        let arg = self.parse_assignment()?;
                        let span = Span::new(start, arg.span.end);
                        let spread = Expr::new(ExprKind::Spread(self.arena.alloc(arg)), span);
                        elements.push(Some(spread));
                    } else {
                        elements.push(Some(self.parse_assignment()?));
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(TokenKind::RBracket)?;
                ExprKind::Array(
                    elements
                        .into_iter()
                        .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
                        .into_bump_slice(),
                )
            }
            TokenKind::LBrace => {
                self.advance();
                let mut props = self.arena.vec();
                while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                    if self.eat(TokenKind::Spread) {
                        // Spread property: ...expr
                        let spread_start = self.current.span.start;
                        let arg = self.parse_assignment()?;
                        let spread_end = arg.span.end;
                        let spread_span = Span::new(spread_start, spread_end);
                        props.push(Property {
                            key: PropertyKey::Ident(self.arena.alloc_str("")),
                            value: Expr::new(ExprKind::Spread(self.arena.alloc(arg)), spread_span),
                            kind: PropertyKind::Init,
                            shorthand: false,
                            computed: false,
                            span: spread_span,
                        });
                    } else {
                        let prop = self.parse_object_property()?;
                        props.push(prop);
                    }
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RBrace)?;
                ExprKind::Object(
                    props
                        .into_iter()
                        .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
                        .into_bump_slice(),
                )
            }
            TokenKind::Function => {
                let func = self.parse_function()?;
                ExprKind::Function(self.arena.alloc(func))
            }
            TokenKind::Import => {
                // Dynamic import expression: import('module')
                self.advance();
                self.expect(TokenKind::LParen)?;
                let arg = self.parse_assignment()?;
                self.expect(TokenKind::RParen)?;
                let end = self.current.span.end;
                return Ok(Expr::new(
                    ExprKind::Import(self.arena.alloc(arg)),
                    Span::new(start, end),
                ));
            }
            TokenKind::TemplateNoSub(_) => {
                return self.parse_template_literal();
            }
            TokenKind::TemplateHead(_) => {
                return self.parse_template_literal();
            }
            _ => {
                return Err(ParseError::new(
                    format!("Unexpected token: {:?}", self.peek()),
                    self.current.span,
                ));
            }
        };

        let end = self.current.span.end;
        Ok(Expr::new(kind, Span::new(start, end)))
    }

    fn parse_object_property(&mut self) -> Result<Property<'a>, ParseError> {
        let start = self.current.span.start;
        let key = self.parse_property_key()?;

        let (value, shorthand) = if self.eat(TokenKind::Colon) {
            (self.parse_assignment()?, false)
        } else {
            // Shorthand property
            let name = match key {
                PropertyKey::Ident(s) => s,
                _ => {
                    return Err(ParseError::new(
                        "Invalid shorthand property",
                        self.current.span,
                    ))
                }
            };
            (
                Expr::new(
                    ExprKind::Ident(name),
                    Span::new(start, self.current.span.end),
                ),
                true,
            )
        };

        let end = self.current.span.end;
        Ok(Property {
            key,
            value,
            kind: PropertyKind::Init,
            shorthand,
            computed: false,
            span: Span::new(start, end),
        })
    }

    // =========================================================================
    // Arrow Functions
    // =========================================================================

    /// Check if we're at the start of an arrow function.
    /// This is a lookahead check without consuming tokens.
    fn is_arrow_function_start(&self) -> bool {
        match self.peek() {
            // Single identifier: x => ...
            // Current token is the identifier, lexer is positioned after it
            TokenKind::Identifier(_) => {
                let mut lookahead = self.lexer.clone();
                // Lexer is already past the identifier, next token should be =>
                matches!(lookahead.next_token().kind, TokenKind::Arrow)
            }
            // Parenthesized: () => ..., (x) => ..., (x, y) => ...
            // Current token is (, lexer is positioned after it
            TokenKind::LParen => {
                let mut lookahead = self.lexer.clone();
                let mut depth = 1;
                // Lexer is already past the (, so start scanning for matching )

                while depth > 0 {
                    match lookahead.next_token().kind {
                        TokenKind::LParen => depth += 1,
                        TokenKind::RParen => depth -= 1,
                        TokenKind::Eof => return false,
                        _ => {}
                    }
                }

                matches!(lookahead.next_token().kind, TokenKind::Arrow)
            }
            _ => false,
        }
    }

    fn parse_arrow_function(&mut self) -> Result<Expr<'a>, ParseError> {
        let start = self.current.span.start;
        let is_async = self.eat(TokenKind::Async);

        // Parse parameters
        let params = if self.is_identifier() {
            // Single identifier: x => ...
            let param_start = self.current.span.start;
            let name = self.parse_ident()?;
            let binding = Binding::new(
                BindingKind::Ident { name },
                Span::new(param_start, self.current.span.end),
            );
            let param = Param {
                binding,
                default: None,
                rest: false,
                span: Span::new(param_start, self.current.span.end),
            };
            let slice: &[Param] = self.arena.alloc_slice_from_iter(std::iter::once(param));
            slice
        } else {
            // Parenthesized: (x, y) => ...
            self.expect(TokenKind::LParen)?;
            let params = self.parse_params()?;
            self.expect(TokenKind::RParen)?;
            params
        };

        self.expect(TokenKind::Arrow)?;

        // Parse body
        let body = if matches!(self.peek(), TokenKind::LBrace) {
            self.expect(TokenKind::LBrace)?;
            let stmts = self.parse_block_body()?;
            self.expect(TokenKind::RBrace)?;
            ArrowBody::Block(stmts)
        } else {
            let expr = self.parse_assignment()?;
            ArrowBody::Expr(self.arena.alloc(expr))
        };

        let end = self.current.span.end;
        let arrow = ArrowFunction {
            params,
            body,
            is_async,
            span: Span::new(start, end),
        };

        Ok(Expr::new(
            ExprKind::Arrow(self.arena.alloc(arrow)),
            Span::new(start, end),
        ))
    }

    // =========================================================================
    // New Expression
    // =========================================================================

    fn parse_new_expr(&mut self) -> Result<Expr<'a>, ParseError> {
        let start = self.current.span.start;
        self.expect(TokenKind::New)?;

        let callee = self.parse_member()?;

        let args = if self.eat(TokenKind::LParen) {
            let args = self.parse_arguments()?;
            self.expect(TokenKind::RParen)?;
            args
        } else {
            &[] as &[Expr]
        };

        let end = self.current.span.end;
        Ok(Expr::new(
            ExprKind::New {
                callee: self.arena.alloc(callee),
                args,
            },
            Span::new(start, end),
        ))
    }

    // =========================================================================
    // Template Literals
    // =========================================================================

    fn parse_template_literal(&mut self) -> Result<Expr<'a>, ParseError> {
        let start = self.current.span.start;

        // Simple template with no substitutions
        if let TokenKind::TemplateNoSub(_) = self.peek() {
            let token = self.advance();
            let s = self.span_text(token.span);
            // Remove backticks
            let s = &s[1..s.len() - 1];
            let end = self.current.span.end;
            return Ok(Expr::new(
                ExprKind::TemplateNoSub(self.arena.alloc_str(s)),
                Span::new(start, end),
            ));
        }

        // Template with substitutions
        let mut quasis = self.arena.vec();
        let mut exprs = self.arena.vec();

        // Parse head
        if let TokenKind::TemplateHead(_) = self.peek() {
            let token = self.advance();
            let s = self.span_text(token.span);
            // Remove ` from start and ${ from end
            let s = &s[1..s.len() - 2];
            quasis.push(self.arena.alloc_str(s));
        } else {
            return Err(ParseError::new("Expected template head", self.current.span));
        }

        loop {
            // Parse expression
            exprs.push(self.parse_expression()?);

            // After expression, we should see RBrace
            // The current token is RBrace, and lexer is positioned right after }
            if !matches!(self.peek(), TokenKind::RBrace) {
                return Err(ParseError::new(
                    format!("Expected '}}' in template literal, got {:?}", self.peek()),
                    self.current.span,
                ));
            }

            // Lexer is already positioned after }, so just scan template continuation
            let cont_start = self.lexer.pos() as u32;
            let cont_kind = self.lexer.scan_template_continuation();
            let cont_end = self.lexer.pos() as u32;

            match &cont_kind {
                TokenKind::TemplateMiddle(s) => {
                    quasis.push(self.arena.alloc_str(s));
                    // Advance to next token for the next expression
                    self.current = self.lexer.next_token();
                }
                TokenKind::TemplateTail(s) => {
                    quasis.push(self.arena.alloc_str(s));
                    // Done with template, get next token
                    self.current = self.lexer.next_token();
                    break;
                }
                _ => {
                    return Err(ParseError::new(
                        format!("Expected template middle or tail, got {:?}", cont_kind),
                        Span::new(cont_start, cont_end),
                    ))
                }
            }
        }

        let end = self.current.span.end;
        let quasis_slice = quasis
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice();
        let exprs_slice = exprs
            .into_iter()
            .collect_in::<bumpalo::collections::Vec<_>>(self.arena.bump())
            .into_bump_slice();

        Ok(Expr::new(
            ExprKind::Template {
                quasis: quasis_slice,
                exprs: exprs_slice,
            },
            Span::new(start, end),
        ))
    }
}

// Helper trait for arena allocation
trait ArenaExt {
    fn alloc_slice_from_iter<T, I: Iterator<Item = T> + ExactSizeIterator>(&self, iter: I) -> &[T];
}

impl ArenaExt for Arena {
    fn alloc_slice_from_iter<T, I: Iterator<Item = T> + ExactSizeIterator>(&self, iter: I) -> &[T] {
        iter.collect_in::<bumpalo::collections::Vec<T>>(self.bump())
            .into_bump_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_parse_simple() {
        let arena = Arena::new();
        let source = "const x = 1 + 2;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_parse_function() {
        let arena = Arena::new();
        let source = "function add(a, b) { return a + b; }";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_memory_usage() {
        let arena = Arena::new();
        let source = "const x = 1; const y = 2; const z = x + y;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let _ = parser.parse().unwrap();

        // All allocations should be in the arena
        let bytes = arena.allocated_bytes();
        assert!(bytes > 0, "Arena should have allocated memory");
        assert!(bytes < 4096, "Simple program should use < 4KB");
    }

    #[test]
    fn test_arena_arrow_function_single_param() {
        let arena = Arena::new();
        let source = "const double = x => x * 2;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_arrow_function_multi_param() {
        let arena = Arena::new();
        let source = "const add = (a, b) => a + b;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_arrow_function_in_call() {
        let arena = Arena::new();
        let source = "nums.map(x => x * 2);";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_arrow_function_block_body() {
        let arena = Arena::new();
        let source = "const fn = (x) => { return x + 1; };";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_class() {
        let arena = Arena::new();
        let source =
            "class Counter { constructor() { this.count = 0; } increment() { this.count++; } }";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_new_expression() {
        let arena = Arena::new();
        let source = "const c = new Counter();";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_complex_program() {
        let arena = Arena::new();
        let source = r#"
            const nums = [1, 2, 3];
            const doubled = nums.map(x => x * 2);
            class Counter {
                constructor() { this.count = 0; }
                increment() { this.count++; }
            }
            const c = new Counter();
        "#;
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 4);
    }

    #[test]
    fn test_arena_import_default() {
        let arena = Arena::new();
        let source = "import foo from 'module';";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_import_named() {
        let arena = Arena::new();
        let source = "import { foo, bar as baz } from 'module';";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_import_namespace() {
        let arena = Arena::new();
        let source = "import * as mod from 'module';";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_import_side_effect() {
        let arena = Arena::new();
        let source = "import 'side-effect';";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_import_combined() {
        let arena = Arena::new();
        let source = "import React, { useState, useEffect } from 'react';";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_export_default() {
        let arena = Arena::new();
        let source = "export default function() {}";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_export_named() {
        let arena = Arena::new();
        let source = "export { foo, bar as baz };";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_export_declaration() {
        let arena = Arena::new();
        let source = "export const x = 1;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_export_all() {
        let arena = Arena::new();
        let source = "export * from 'module';";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_full_module() {
        let arena = Arena::new();
        let source = r#"
            import React, { useState } from 'react';
            import * as utils from './utils';

            const Counter = () => {
                const count = useState(0);
                return count;
            };

            export default Counter;
            export { utils };
        "#;
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 5);
    }

    #[test]
    fn test_arena_template_simple() {
        let arena = Arena::new();
        let source = "const s = `hello world`;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_template_with_expression() {
        let arena = Arena::new();
        let source = "const s = `hello ${name}`;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_template_multiple_expressions() {
        let arena = Arena::new();
        let source = "const s = `${a} + ${b} = ${a + b}`;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_array_destructuring() {
        let arena = Arena::new();
        let source = "const [a, b, c] = arr;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_array_destructuring_with_rest() {
        let arena = Arena::new();
        let source = "const [first, ...rest] = arr;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_array_destructuring_with_default() {
        let arena = Arena::new();
        let source = "const [a = 1, b = 2] = arr;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_object_destructuring() {
        let arena = Arena::new();
        let source = "const { x, y } = obj;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_object_destructuring_rename() {
        let arena = Arena::new();
        let source = "const { x: newX, y: newY } = obj;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_object_destructuring_with_rest() {
        let arena = Arena::new();
        let source = "const { x, ...rest } = obj;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_object_destructuring_with_default() {
        let arena = Arena::new();
        let source = "const { x = 1, y = 2 } = obj;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_nested_destructuring() {
        let arena = Arena::new();
        let source = "const { a: [b, c], d: { e, f } } = obj;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_destructuring_in_function_params() {
        let arena = Arena::new();
        let source = "function foo({ x, y }, [a, b]) { return x + y + a + b; }";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_spread_in_array() {
        let arena = Arena::new();
        let source = "const arr = [...a, ...b, c];";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_spread_in_call() {
        let arena = Arena::new();
        let source = "fn(...args);";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_spread_in_object() {
        let arena = Arena::new();
        let source = "const obj = { ...a, ...b, c: 1 };";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_spread_combined() {
        let arena = Arena::new();
        let source = "const result = merge({ ...defaults, ...options }, [...items, newItem]);";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_switch() {
        let arena = Arena::new();
        let source = r#"
            switch (x) {
                case 1:
                    break;
                case 2:
                case 3:
                    return;
                default:
                    throw new Error();
            }
        "#;
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_do_while() {
        let arena = Arena::new();
        let source = "do { x++; } while (x < 10);";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_try_catch() {
        let arena = Arena::new();
        let source = "try { foo(); } catch (e) { console.log(e); }";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_try_catch_finally() {
        let arena = Arena::new();
        let source = "try { foo(); } catch (e) { log(e); } finally { cleanup(); }";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_try_finally() {
        let arena = Arena::new();
        let source = "try { foo(); } finally { cleanup(); }";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_throw() {
        let arena = Arena::new();
        let source = "throw new Error('oops');";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_break_continue() {
        let arena = Arena::new();
        let source = "while (true) { if (x) break; if (y) continue; }";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_debugger() {
        let arena = Arena::new();
        let source = "debugger;";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_for_in() {
        let arena = Arena::new();
        let source = "for (const key in obj) { console.log(key); }";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_for_of() {
        let arena = Arena::new();
        let source = "for (const item of items) { console.log(item); }";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }

    #[test]
    fn test_arena_for_of_destructuring() {
        let arena = Arena::new();
        let source = "for (const [key, value] of entries) { console.log(key, value); }";
        let parser = ArenaParser::new(&arena, source, ParserOptions::default());
        let program = parser.parse().unwrap();
        assert_eq!(program.stmts.len(), 1);
    }
}
