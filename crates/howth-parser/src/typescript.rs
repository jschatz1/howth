//! TypeScript-specific parsing.
//!
//! All TS parsing as `impl<'a> Parser<'a>` methods (same pattern as jsx.rs).
//! Handles type annotations, type declarations, generics, access modifiers,
//! namespaces/modules, and declare blocks.

use crate::ast::*;
use crate::parser::{ParseError, Parser};
use crate::span::Span;
use crate::token::{Token, TokenKind};

/// Check if a token is a TypeScript keyword that can also be used as an identifier.
/// These are contextual keywords - they're only keywords in type positions.
pub fn is_ts_contextual_keyword(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Type
            | TokenKind::Interface
            | TokenKind::Enum
            | TokenKind::Namespace
            | TokenKind::Module
            | TokenKind::Declare
            | TokenKind::Abstract
            | TokenKind::Readonly
            | TokenKind::Override
            | TokenKind::Keyof
            | TokenKind::Infer
            | TokenKind::Any
            | TokenKind::Unknown
            | TokenKind::Never
            | TokenKind::Asserts
            | TokenKind::Satisfies
            | TokenKind::Is
            | TokenKind::Public
            | TokenKind::Private
            | TokenKind::Protected
            | TokenKind::Implements
    )
}

impl<'a> Parser<'a> {
    // =========================================================================
    // Type Parsing
    // =========================================================================

    /// Entry point for parsing a TypeScript type.
    pub(crate) fn parse_ts_type_impl(&mut self) -> Result<TsType, ParseError> {
        let ty = self.parse_ts_conditional_type()?;
        // Type predicate: `x is Type` — treat as the RHS type (since we strip types)
        if self.check(&TokenKind::Is) {
            self.advance();
            let rhs = self.parse_ts_type_impl()?;
            return Ok(rhs);
        }
        Ok(ty)
    }

    /// Parse a conditional type: `T extends U ? V : W`
    fn parse_ts_conditional_type(&mut self) -> Result<TsType, ParseError> {
        let start = self.current.span.start;
        let ty = self.parse_ts_union_type()?;

        if self.check(&TokenKind::Extends) {
            self.advance();
            let extends = self.parse_ts_union_type()?;
            self.expect(&TokenKind::Question)?;
            let true_type = self.parse_ts_type_impl()?;
            self.expect(&TokenKind::Colon)?;
            let false_type = self.parse_ts_type_impl()?;
            let end = self.current.span.start;
            return Ok(TsType {
                kind: TsTypeKind::Conditional {
                    check: Box::new(ty),
                    extends: Box::new(extends),
                    true_type: Box::new(true_type),
                    false_type: Box::new(false_type),
                },
                span: Span::new(start, end),
            });
        }

        Ok(ty)
    }

    /// Parse a union type: `A | B | C`
    fn parse_ts_union_type(&mut self) -> Result<TsType, ParseError> {
        let start = self.current.span.start;
        // Allow leading `|`
        self.eat(&TokenKind::Pipe);
        let first = self.parse_ts_intersection_type()?;

        if !self.check(&TokenKind::Pipe) {
            return Ok(first);
        }

        let mut types = vec![first];
        while self.eat(&TokenKind::Pipe) {
            types.push(self.parse_ts_intersection_type()?);
        }
        let end = self.current.span.start;
        Ok(TsType {
            kind: TsTypeKind::Union(types),
            span: Span::new(start, end),
        })
    }

    /// Parse an intersection type: `A & B & C`
    fn parse_ts_intersection_type(&mut self) -> Result<TsType, ParseError> {
        let start = self.current.span.start;
        // Allow leading `&`
        self.eat(&TokenKind::Amp);
        let first = self.parse_ts_postfix_type()?;

        if !self.check(&TokenKind::Amp) {
            return Ok(first);
        }

        let mut types = vec![first];
        while self.eat(&TokenKind::Amp) {
            types.push(self.parse_ts_postfix_type()?);
        }
        let end = self.current.span.start;
        Ok(TsType {
            kind: TsTypeKind::Intersection(types),
            span: Span::new(start, end),
        })
    }

    /// Parse postfix types: `T[]`, `T[K]`
    fn parse_ts_postfix_type(&mut self) -> Result<TsType, ParseError> {
        let start = self.current.span.start;
        let mut ty = self.parse_ts_primary_type()?;

        loop {
            // Don't consume `[` on a new line — it's likely a new member (ASI in interfaces)
            if self.check(&TokenKind::LBracket) && !self.current.had_newline_before {
                self.advance();
                if self.check(&TokenKind::RBracket) {
                    // Array type: T[]
                    self.advance();
                    let end = self.current.span.start;
                    ty = TsType {
                        kind: TsTypeKind::Array(Box::new(ty)),
                        span: Span::new(start, end),
                    };
                } else {
                    // Indexed access: T[K]
                    let index = self.parse_ts_type_impl()?;
                    self.expect(&TokenKind::RBracket)?;
                    let end = self.current.span.start;
                    ty = TsType {
                        kind: TsTypeKind::Indexed {
                            object: Box::new(ty),
                            index: Box::new(index),
                        },
                        span: Span::new(start, end),
                    };
                }
            } else {
                break;
            }
        }

        Ok(ty)
    }

    /// Parse a primary type.
    fn parse_ts_primary_type(&mut self) -> Result<TsType, ParseError> {
        let start = self.current.span.start;

        match self.peek().clone() {
            // Keyword types
            TokenKind::Any => {
                self.advance();
                Ok(TsType { kind: TsTypeKind::Any, span: Span::new(start, self.current.span.start) })
            }
            TokenKind::Unknown => {
                self.advance();
                Ok(TsType { kind: TsTypeKind::Unknown, span: Span::new(start, self.current.span.start) })
            }
            TokenKind::Never => {
                self.advance();
                Ok(TsType { kind: TsTypeKind::Never, span: Span::new(start, self.current.span.start) })
            }
            TokenKind::Void => {
                self.advance();
                Ok(TsType { kind: TsTypeKind::Void, span: Span::new(start, self.current.span.start) })
            }
            TokenKind::Null => {
                self.advance();
                Ok(TsType { kind: TsTypeKind::Null, span: Span::new(start, self.current.span.start) })
            }
            TokenKind::This => {
                self.advance();
                Ok(TsType { kind: TsTypeKind::This, span: Span::new(start, self.current.span.start) })
            }
            TokenKind::Typeof => {
                self.advance();
                // `typeof import("module")` — dynamic import in typeof position
                if self.check(&TokenKind::Import) {
                    let import_type = self.parse_ts_primary_type()?;
                    let end = self.current.span.start;
                    return Ok(TsType { kind: TsTypeKind::Typeof(Box::new(
                        Expr::new(ExprKind::Ident("import".to_string()), import_type.span)
                    )), span: Span::new(start, end) });
                }
                // Parse a qualified name: `Foo.bar.baz` — NOT full expressions
                // This prevents `typeof X[K]` from being parsed as computed member access
                let expr = self.parse_ts_typeof_operand()?;
                let end = self.current.span.start;
                Ok(TsType { kind: TsTypeKind::Typeof(Box::new(expr)), span: Span::new(start, end) })
            }
            TokenKind::Keyof => {
                self.advance();
                let ty = self.parse_ts_postfix_type()?;
                let end = self.current.span.start;
                Ok(TsType { kind: TsTypeKind::Keyof(Box::new(ty)), span: Span::new(start, end) })
            }
            TokenKind::Infer => {
                self.advance();
                let name = self.expect_ts_identifier()?;
                let constraint = if self.check(&TokenKind::Extends) {
                    self.advance();
                    Some(Box::new(self.parse_ts_primary_type()?))
                } else {
                    None
                };
                let end = self.current.span.start;
                Ok(TsType {
                    kind: TsTypeKind::Infer {
                        param: TsTypeParam {
                            name,
                            constraint,
                            default: None,
                            span: Span::new(start, end),
                        },
                    },
                    span: Span::new(start, end),
                })
            }
            TokenKind::Readonly => {
                // readonly before tuple/array
                self.advance();
                let ty = self.parse_ts_postfix_type()?;
                // Just return the inner type - readonly is a modifier, not represented as separate kind
                Ok(ty)
            }

            // `asserts x` or `asserts x is Type` — type predicate assertion
            TokenKind::Asserts => {
                self.advance();
                // Consume the asserted parameter name
                if matches!(self.peek(), TokenKind::Identifier(_) | TokenKind::This) {
                    self.advance();
                }
                // Optional `is Type`
                if self.check(&TokenKind::Is) {
                    self.advance();
                    let _ = self.parse_ts_type_impl()?;
                }
                let end = self.current.span.start;
                Ok(TsType { kind: TsTypeKind::Any, span: Span::new(start, end) })
            }

            // Identifier - could be type reference, or keyword-like types
            TokenKind::Identifier(ref name) => {
                let name_clone = name.clone();
                match name_clone.as_str() {
                    "undefined" => {
                        self.advance();
                        Ok(TsType { kind: TsTypeKind::Undefined, span: Span::new(start, self.current.span.start) })
                    }
                    "boolean" => {
                        self.advance();
                        Ok(TsType { kind: TsTypeKind::Boolean, span: Span::new(start, self.current.span.start) })
                    }
                    "number" => {
                        self.advance();
                        Ok(TsType { kind: TsTypeKind::Number, span: Span::new(start, self.current.span.start) })
                    }
                    "string" => {
                        self.advance();
                        Ok(TsType { kind: TsTypeKind::String, span: Span::new(start, self.current.span.start) })
                    }
                    "symbol" => {
                        self.advance();
                        Ok(TsType { kind: TsTypeKind::Symbol, span: Span::new(start, self.current.span.start) })
                    }
                    "unique" => {
                        // `unique symbol` type
                        self.advance();
                        if matches!(self.peek(), TokenKind::Identifier(ref n) if n == "symbol") {
                            self.advance();
                        }
                        Ok(TsType { kind: TsTypeKind::Symbol, span: Span::new(start, self.current.span.start) })
                    }
                    "bigint" => {
                        self.advance();
                        Ok(TsType { kind: TsTypeKind::BigInt, span: Span::new(start, self.current.span.start) })
                    }
                    "object" => {
                        self.advance();
                        Ok(TsType { kind: TsTypeKind::Object, span: Span::new(start, self.current.span.start) })
                    }
                    _ => {
                        // Type reference: Foo, Foo<T>
                        self.parse_ts_type_reference()
                    }
                }
            }

            // Literal types
            TokenKind::String(s) => {
                self.advance();
                let end = self.current.span.start;
                Ok(TsType { kind: TsTypeKind::LitString(s), span: Span::new(start, end) })
            }
            TokenKind::Number(n) => {
                self.advance();
                let end = self.current.span.start;
                Ok(TsType { kind: TsTypeKind::LitNumber(n), span: Span::new(start, end) })
            }
            TokenKind::True => {
                self.advance();
                Ok(TsType { kind: TsTypeKind::LitBoolean(true), span: Span::new(start, self.current.span.start) })
            }
            TokenKind::False => {
                self.advance();
                Ok(TsType { kind: TsTypeKind::LitBoolean(false), span: Span::new(start, self.current.span.start) })
            }
            TokenKind::Minus => {
                // Negative number literal type: -1
                self.advance();
                if let TokenKind::Number(n) = self.peek().clone() {
                    self.advance();
                    let end = self.current.span.start;
                    Ok(TsType { kind: TsTypeKind::LitNumber(-n), span: Span::new(start, end) })
                } else {
                    Err(ParseError::new("Expected number after '-' in type", self.current.span))
                }
            }

            // Parenthesized type or function type: `(x: number) => void` or `(Type)`
            TokenKind::LParen => {
                self.parse_ts_paren_or_fn_type()
            }

            // Tuple type: `[A, B, C]`
            TokenKind::LBracket => {
                self.parse_ts_tuple_type()
            }

            // Object type literal: `{ x: number; y: string }`
            TokenKind::LBrace => {
                self.parse_ts_type_literal()
            }

            // Template literal type: `\`hello\``
            TokenKind::TemplateNoSub(s) => {
                self.advance();
                let end = self.current.span.start;
                Ok(TsType {
                    kind: TsTypeKind::Template { quasis: vec![s], types: Vec::new() },
                    span: Span::new(start, end),
                })
            }
            // Template literal type with interpolation: `\`hello ${Type}\``
            TokenKind::TemplateHead(s) => {
                let mut quasis = vec![s];
                let mut types = Vec::new();
                self.advance();
                loop {
                    types.push(self.parse_ts_type_impl()?);
                    // After the type, current token should be `}` closing the ${...}
                    if !matches!(self.peek(), TokenKind::RBrace) {
                        return Err(ParseError::new("Expected } in template literal type", self.current.span));
                    }
                    // Scan template continuation from the lexer
                    let cont_kind = self.lexer.scan_template_continuation();
                    self.current = Token::new(cont_kind, self.current.span);
                    match self.peek().clone() {
                        TokenKind::TemplateTail(s) => {
                            quasis.push(s);
                            self.advance();
                            break;
                        }
                        TokenKind::TemplateMiddle(s) => {
                            quasis.push(s);
                            self.advance();
                        }
                        _ => break,
                    }
                }
                let end = self.current.span.start;
                Ok(TsType {
                    kind: TsTypeKind::Template { quasis, types },
                    span: Span::new(start, end),
                })
            }
            // BigInt literal type
            TokenKind::BigInt(n) => {
                let n = n.clone();
                self.advance();
                let end = self.current.span.start;
                Ok(TsType { kind: TsTypeKind::LitString(n), span: Span::new(start, end) })
            }

            // `new (...) => T` constructor type
            TokenKind::New => {
                self.advance();
                let type_params = if self.check(&TokenKind::Lt) {
                    Some(self.parse_ts_type_params_impl()?)
                } else {
                    None
                };
                self.expect(&TokenKind::LParen)?;
                let params = self.parse_ts_fn_params_impl()?;
                self.expect(&TokenKind::RParen)?;
                self.expect(&TokenKind::Arrow)?;
                let return_type = self.parse_ts_type_impl()?;
                let end = self.current.span.start;
                Ok(TsType {
                    kind: TsTypeKind::Constructor(Box::new(TsFunctionType {
                        params,
                        type_params,
                        return_type: Box::new(return_type),
                    })),
                    span: Span::new(start, end),
                })
            }

            // `abstract new (...) => T` — abstract constructor type
            TokenKind::Abstract if matches!(self.lexer.peek().kind, TokenKind::New) => {
                self.advance(); // consume `abstract`
                // Now parse as constructor type (same as `new` branch)
                self.advance(); // consume `new`
                let type_params = if self.check(&TokenKind::Lt) {
                    Some(self.parse_ts_type_params_impl()?)
                } else {
                    None
                };
                self.expect(&TokenKind::LParen)?;
                let params = self.parse_ts_fn_params_impl()?;
                self.expect(&TokenKind::RParen)?;
                self.expect(&TokenKind::Arrow)?;
                let return_type = self.parse_ts_type_impl()?;
                let end = self.current.span.start;
                Ok(TsType {
                    kind: TsTypeKind::Constructor(Box::new(TsFunctionType {
                        params,
                        type_params,
                        return_type: Box::new(return_type),
                    })),
                    span: Span::new(start, end),
                })
            }

            // TS keyword types used as identifiers (when they clash with token kinds)
            ref k if is_ts_contextual_keyword(k) => {
                // These TS keywords can also be type references
                let name = self.expect_ts_identifier()?;
                let type_args = if self.check(&TokenKind::Lt) {
                    Some(self.parse_ts_type_args_impl()?)
                } else {
                    None
                };
                let end = self.current.span.start;
                Ok(TsType {
                    kind: TsTypeKind::Reference { name, type_args },
                    span: Span::new(start, end),
                })
            }

            // JS contextual keywords used as type names
            TokenKind::From | TokenKind::Get | TokenKind::Set | TokenKind::As
            | TokenKind::Static | TokenKind::Async => {
                let name = match self.peek() {
                    TokenKind::From => "from",
                    TokenKind::Get => "get",
                    TokenKind::Set => "set",
                    TokenKind::As => "as",
                    TokenKind::Static => "static",
                    TokenKind::Async => "async",
                    _ => unreachable!(),
                }.to_string();
                self.advance();
                let type_args = if self.check(&TokenKind::Lt) {
                    Some(self.parse_ts_type_args_impl()?)
                } else {
                    None
                };
                let end = self.current.span.start;
                Ok(TsType {
                    kind: TsTypeKind::Reference { name, type_args },
                    span: Span::new(start, end),
                })
            }
            // TS contextual keywords used as type names
            _ if is_ts_contextual_keyword(self.peek()) => {
                let name = self.expect_ts_identifier()?;
                let type_args = if self.check(&TokenKind::Lt) {
                    Some(self.parse_ts_type_args_impl()?)
                } else {
                    None
                };
                let end = self.current.span.start;
                Ok(TsType {
                    kind: TsTypeKind::Reference { name, type_args },
                    span: Span::new(start, end),
                })
            }
            // import("./module").Type
            TokenKind::Import => {
                self.advance();
                self.expect(&TokenKind::LParen)?;
                let _ = self.parse_assign_expr()?; // module path
                self.expect(&TokenKind::RParen)?;
                // optional .Type
                if self.eat(&TokenKind::Dot) {
                    let name = if let TokenKind::Identifier(n) = self.peek() {
                        let n = n.clone();
                        self.advance();
                        n
                    } else {
                        "default".to_string()
                    };
                    let type_args = if self.check(&TokenKind::Lt) {
                        Some(self.parse_ts_type_args_impl()?)
                    } else {
                        None
                    };
                    let end = self.current.span.start;
                    Ok(TsType {
                        kind: TsTypeKind::Reference { name, type_args },
                        span: Span::new(start, end),
                    })
                } else {
                    let end = self.current.span.start;
                    Ok(TsType { kind: TsTypeKind::Any, span: Span::new(start, end) })
                }
            }
            // Spread type in tuples: `...T`
            TokenKind::Spread => {
                self.advance();
                let ty = self.parse_ts_type_impl()?;
                let end = self.current.span.start;
                Ok(TsType { kind: ty.kind, span: Span::new(start, end) })
            }
            // Generic function type: `<T>(x: T) => T`
            TokenKind::Lt => {
                let type_params = Some(self.parse_ts_type_params_impl()?);
                self.expect(&TokenKind::LParen)?;
                let params = self.parse_ts_fn_params_impl()?;
                self.expect(&TokenKind::RParen)?;
                self.expect(&TokenKind::Arrow)?;
                let return_type = self.parse_ts_type_impl()?;
                let end = self.current.span.start;
                Ok(TsType {
                    kind: TsTypeKind::Function(Box::new(TsFunctionType {
                        params,
                        type_params,
                        return_type: Box::new(return_type),
                    })),
                    span: Span::new(start, end),
                })
            }
            _ => Err(ParseError::new(
                format!("Expected type, got {:?}", self.peek()),
                self.current.span,
            )),
        }
    }

    /// Parse a type reference: `Foo`, `Foo<T>`, `Foo.Bar`
    fn parse_ts_type_reference(&mut self) -> Result<TsType, ParseError> {
        let start = self.current.span.start;
        let name = self.expect_ts_identifier()?;

        // Check for qualified name: Foo.Bar
        if self.check(&TokenKind::Dot) {
            let mut ty = TsType {
                kind: TsTypeKind::Reference { name, type_args: None },
                span: Span::new(start, self.current.span.start),
            };
            while self.eat(&TokenKind::Dot) {
                let right = self.expect_ts_identifier()?;
                let end = self.current.span.start;
                ty = TsType {
                    kind: TsTypeKind::Qualified { left: Box::new(ty), right },
                    span: Span::new(start, end),
                };
            }
            // Type args on the final qualified name — consume them even though we don't store them
            if self.check(&TokenKind::Lt) {
                let _ = self.parse_ts_type_args_impl()?;
            }
            return Ok(ty);
        }

        // Type args: Foo<T, U>
        let type_args = if self.check(&TokenKind::Lt) {
            Some(self.parse_ts_type_args_impl()?)
        } else {
            None
        };

        let end = self.current.span.start;
        Ok(TsType {
            kind: TsTypeKind::Reference { name, type_args },
            span: Span::new(start, end),
        })
    }

    /// Parse parenthesized type or function type.
    /// Ambiguity: `(x: number) => void` vs `(number)`
    fn parse_ts_paren_or_fn_type(&mut self) -> Result<TsType, ParseError> {
        let start = self.current.span.start;

        self.expect(&TokenKind::LParen)?;

        // Empty parens → function type: `() => T`
        if self.check(&TokenKind::RParen) {
            self.advance();
            if self.check(&TokenKind::Arrow) {
                self.advance();
                let return_type = self.parse_ts_type_impl()?;
                let end = self.current.span.start;
                return Ok(TsType {
                    kind: TsTypeKind::Function(Box::new(TsFunctionType {
                        params: Vec::new(),
                        type_params: None,
                        return_type: Box::new(return_type),
                    })),
                    span: Span::new(start, end),
                });
            }
            // `()` — treat as void
            let end = self.current.span.start;
            return Ok(TsType {
                kind: TsTypeKind::Void,
                span: Span::new(start, end),
            });
        }

        // Heuristic to determine if this is a function type or parenthesized type:
        // - `...` → rest param → function type
        // - `identifier :` or `identifier ?` or `identifier ,` → function type param
        // Otherwise parse as parenthesized type and check for `=>` after.
        let is_fn_type = match self.peek() {
            TokenKind::Spread => true,
            TokenKind::This => {
                // `(this: T, ...)` — `this` parameter in function type
                let next = self.lexer.peek();
                matches!(next.kind, TokenKind::Colon | TokenKind::Question | TokenKind::Comma)
            }
            TokenKind::Identifier(_) => {
                // Peek at the token after the identifier using lexer.peek()
                let next = self.lexer.peek();
                matches!(next.kind, TokenKind::Colon | TokenKind::Question | TokenKind::Comma)
            }
            // Keywords used as parameter names: `(from: Type, to: Type) => void`
            ref k if k.is_keyword() || is_ts_contextual_keyword(k) => {
                let next = self.lexer.peek();
                matches!(next.kind, TokenKind::Colon | TokenKind::Question)
            }
            _ => false,
        };

        if is_fn_type {
            // Parse as function type params
            let params = self.parse_ts_fn_params_impl()?;
            self.expect(&TokenKind::RParen)?;
            if self.check(&TokenKind::Arrow) {
                self.advance();
                let return_type = self.parse_ts_type_impl()?;
                let end = self.current.span.start;
                return Ok(TsType {
                    kind: TsTypeKind::Function(Box::new(TsFunctionType {
                        params,
                        type_params: None,
                        return_type: Box::new(return_type),
                    })),
                    span: Span::new(start, end),
                });
            }
            // If no arrow, it was actually a parenthesized type with params syntax
            // Fall back - use the first param's type
            if params.len() == 1 {
                let end = self.current.span.start;
                return Ok(TsType {
                    kind: TsTypeKind::Parenthesized(Box::new(params.into_iter().next().unwrap().ty)),
                    span: Span::new(start, end),
                });
            }
        }

        // Parse as parenthesized type
        let inner = self.parse_ts_type_impl()?;
        self.expect(&TokenKind::RParen)?;

        // Check for arrow (function type with single unnamed param)
        if self.check(&TokenKind::Arrow) {
            self.advance();
            let return_type = self.parse_ts_type_impl()?;
            let end = self.current.span.start;
            return Ok(TsType {
                kind: TsTypeKind::Function(Box::new(TsFunctionType {
                    params: vec![TsFnParam {
                        name: None,
                        ty: inner,
                        optional: false,
                        rest: false,
                    }],
                    type_params: None,
                    return_type: Box::new(return_type),
                })),
                span: Span::new(start, end),
            });
        }

        let end = self.current.span.start;
        Ok(TsType {
            kind: TsTypeKind::Parenthesized(Box::new(inner)),
            span: Span::new(start, end),
        })
    }

    /// Parse a tuple type: `[A, B, C]` or labeled `[name: Type, age: Type]`
    fn parse_ts_tuple_type(&mut self) -> Result<TsType, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::LBracket)?;

        let mut types = Vec::new();
        while !self.check(&TokenKind::RBracket) && !self.is_eof() {
            // Handle optional spread: `...Type`
            let has_spread = self.eat(&TokenKind::Spread);

            // Check for labeled tuple: `name: Type` or `name?: Type`
            // Must distinguish from optional type element: `string?`
            let is_labeled = match self.peek() {
                TokenKind::Identifier(_) => {
                    let next = self.lexer.peek();
                    match &next.kind {
                        TokenKind::Colon => true,
                        TokenKind::Question => {
                            // `name?: type` vs `string?` — check if `:` follows `?`
                            let saved = self.lexer.clone();
                            let _ = self.lexer.next_token(); // skip past Question
                            let third = self.lexer.peek();
                            let result = matches!(third.kind, TokenKind::Colon);
                            self.lexer = saved;
                            result
                        }
                        _ => false,
                    }
                }
                _ => false,
            };

            if is_labeled {
                // Consume label name
                self.advance();
                // Consume optional `?`
                self.eat(&TokenKind::Question);
                // Consume `:`
                self.expect(&TokenKind::Colon)?;
            }

            let ty = self.parse_ts_type_impl()?;
            let _ = has_spread; // spread is consumed, type represents the rest element
            // Optional tuple element: `string?`
            self.eat(&TokenKind::Question);
            types.push(ty);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBracket)?;
        let end = self.current.span.start;

        Ok(TsType {
            kind: TsTypeKind::Tuple(types),
            span: Span::new(start, end),
        })
    }

    /// Parse an object type literal: `{ x: number; y: string }`
    fn parse_ts_type_literal(&mut self) -> Result<TsType, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::LBrace)?;

        let mut members = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            members.push(self.parse_ts_type_member_impl()?);
            // Members separated by `;` or `,`
            if !self.eat(&TokenKind::Semicolon) {
                self.eat(&TokenKind::Comma);
            }
        }
        self.expect(&TokenKind::RBrace)?;
        let end = self.current.span.start;

        Ok(TsType {
            kind: TsTypeKind::TypeLiteral(members),
            span: Span::new(start, end),
        })
    }

    // =========================================================================
    // Type Parameters and Arguments
    // =========================================================================

    /// Parse type parameters: `<T, U extends V = W>`
    pub(crate) fn parse_ts_type_params_impl(&mut self) -> Result<Vec<TsTypeParam>, ParseError> {
        self.expect(&TokenKind::Lt)?;
        let mut params = Vec::new();

        while !self.check(&TokenKind::Gt)
            && !self.check(&TokenKind::GtGt)
            && !self.check(&TokenKind::GtGtGt)
            && !self.is_eof()
        {
            let param_start = self.current.span.start;
            // TypeScript 5.0: `const` modifier on type params: `<const T>`
            self.eat(&TokenKind::Const);
            // Variance annotations: `in` and `out`
            if matches!(self.peek(), TokenKind::In) {
                self.advance();
            }
            if matches!(self.peek(), TokenKind::Identifier(ref n) if n == "out") {
                self.advance();
            }
            let name = self.expect_ts_identifier()?;

            let constraint = if self.check(&TokenKind::Extends) {
                self.advance();
                Some(Box::new(self.parse_ts_type_impl()?))
            } else {
                None
            };

            let default = if self.eat(&TokenKind::Eq) {
                Some(Box::new(self.parse_ts_type_impl()?))
            } else {
                None
            };

            let param_end = self.current.span.start;
            params.push(TsTypeParam {
                name,
                constraint,
                default,
                span: Span::new(param_start, param_end),
            });

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        self.expect_ts_gt()?;
        Ok(params)
    }

    /// Parse type arguments: `<number, string>`
    pub(crate) fn parse_ts_type_args_impl(&mut self) -> Result<Vec<TsType>, ParseError> {
        self.expect(&TokenKind::Lt)?;
        let mut args = Vec::new();

        while !self.check(&TokenKind::Gt)
            && !self.check(&TokenKind::GtGt)
            && !self.check(&TokenKind::GtGtGt)
            && !self.is_eof()
        {
            args.push(self.parse_ts_type_impl()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        self.expect_ts_gt()?;
        Ok(args)
    }

    /// Expect a `>` token, splitting `>>` or `>>>` if needed for nested generics.
    fn expect_ts_gt(&mut self) -> Result<(), ParseError> {
        if self.check(&TokenKind::Gt) {
            self.advance();
            Ok(())
        } else if self.check(&TokenKind::GtGt) {
            // Split >> into > and leave one > as current
            self.current = Token::new(TokenKind::Gt, Span::new(
                self.current.span.start + 1,
                self.current.span.end,
            ));
            Ok(())
        } else if self.check(&TokenKind::GtGtGt) {
            // Split >>> into > and leave >> as current
            self.current = Token::new(TokenKind::GtGt, Span::new(
                self.current.span.start + 1,
                self.current.span.end,
            ));
            Ok(())
        } else {
            Err(ParseError::new(
                format!("Expected '>', got {:?}", self.peek()),
                self.current.span,
            ))
        }
    }

    /// Parse function type parameters: `(x: number, y?: string)`
    pub(crate) fn parse_ts_fn_params_impl(&mut self) -> Result<Vec<TsFnParam>, ParseError> {
        let mut params = Vec::new();

        while !self.check(&TokenKind::RParen) && !self.is_eof() {
            // Consume decorators on params
            while self.eat(&TokenKind::At) {
                let _ = self.parse_left_hand_side_expr()?;
            }
            // Consume accessibility modifiers (public/private/protected) and readonly
            self.try_parse_accessibility();
            self.eat(&TokenKind::Readonly);

            let rest = self.eat(&TokenKind::Spread);

            // Check if the current token looks like a parameter name
            // (identifier, `this`, or a contextual keyword followed by `:` or `?`)
            let name = if let TokenKind::Identifier(_) = self.peek() {
                let n = self.expect_ts_identifier()?;
                Some(n)
            } else if self.check(&TokenKind::This) {
                self.advance();
                Some("this".to_string())
            } else if (self.peek().is_keyword() || is_ts_contextual_keyword(self.peek()))
                && matches!(self.lexer.peek().kind, TokenKind::Colon | TokenKind::Question)
            {
                // Keywords used as parameter names: `from: Type`, `type: Type`, etc.
                let name = match self.expect_ts_identifier() {
                    Ok(n) => n,
                    Err(_) => {
                        // Hard keyword used as param name — just skip it
                        let n = crate::parser::keyword_to_str(self.peek()).to_string();
                        self.advance();
                        n
                    }
                };
                Some(name)
            } else {
                None
            };

            let optional = self.eat(&TokenKind::Question);

            // Consume `:` before type
            let ty = if self.eat(&TokenKind::Colon) {
                self.parse_ts_type_impl()?
            } else if let Some(ref n) = name {
                // No colon — the name IS the type (e.g., `(number)` not `(x: number)`)
                TsType {
                    kind: TsTypeKind::Reference { name: n.clone(), type_args: None },
                    span: self.current.span,
                }
            } else {
                return Err(ParseError::new("Expected type in function type parameter", self.current.span));
            };

            params.push(TsFnParam {
                name: if self.check(&TokenKind::Colon) || name.is_some() { name } else { None },
                ty,
                optional,
                rest,
            });

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        Ok(params)
    }

    /// Parse a type member (property, method, index, call, construct signature).
    pub(crate) fn parse_ts_type_member_impl(&mut self) -> Result<TsTypeMember, ParseError> {
        let start = self.current.span.start;

        // Check for +/- modifier before readonly: `+readonly`, `-readonly`
        let _modifier = if self.check(&TokenKind::Plus) || self.check(&TokenKind::Minus) {
            if matches!(self.lexer.peek().kind, TokenKind::Readonly) {
                self.advance(); // eat +/-
                true
            } else {
                false
            }
        } else {
            false
        };

        // Check for readonly — but only consume as modifier, not as property name
        let readonly = self.check(&TokenKind::Readonly) && !self.is_ts_modifier_used_as_property() && self.eat(&TokenKind::Readonly);

        // `[...]` — index signature, mapped type, or computed property key
        if self.check(&TokenKind::LBracket) {
            self.advance();

            // Try to identify what kind of `[...]` this is:
            // - Index signature: `[key: Type]: Value` or `[key?: Type]: Value`
            // - Mapped type: `[K in Type]: Value`
            // - Computed property: `[expr]: Type` or `[expr](): Type`
            let is_index_or_mapped = {
                let could_be_ident = matches!(self.peek(),
                    TokenKind::Identifier(_)
                    | TokenKind::Type | TokenKind::Readonly | TokenKind::Interface
                    | TokenKind::Namespace | TokenKind::Module | TokenKind::Declare
                    | TokenKind::Abstract | TokenKind::Override | TokenKind::Any
                    | TokenKind::Unknown | TokenKind::Never | TokenKind::Is
                    | TokenKind::Asserts | TokenKind::Satisfies | TokenKind::Keyof
                    | TokenKind::Infer | TokenKind::Enum | TokenKind::Async
                    | TokenKind::From | TokenKind::Get | TokenKind::Set
                    | TokenKind::As | TokenKind::Static
                );
                if could_be_ident {
                    let next = self.lexer.peek();
                    matches!(next.kind, TokenKind::Colon | TokenKind::Question | TokenKind::In)
                } else {
                    false
                }
            };

            if is_index_or_mapped {
                let param_name = self.expect_ts_identifier()?;

                // Mapped type: `[K in keyof T]` or `[K in T as V]`
                if self.check(&TokenKind::In) {
                    self.advance();
                    let _constraint = self.parse_ts_type_impl()?;
                    // Optional `as` clause: `[K in T as V]`
                    if self.eat(&TokenKind::As) {
                        let _ = self.parse_ts_type_impl()?;
                    }
                    self.expect(&TokenKind::RBracket)?;
                    // Optional `?`, `-?`, or `+?` modifier
                    if self.eat(&TokenKind::Minus) || self.eat(&TokenKind::Plus) {
                        self.eat(&TokenKind::Question);
                    } else {
                        self.eat(&TokenKind::Question);
                    }
                    self.expect(&TokenKind::Colon)?;
                    let type_ann = self.parse_ts_type_impl()?;
                    let end = self.current.span.start;
                    return Ok(TsTypeMember {
                        kind: TsTypeMemberKind::Index {
                            readonly,
                            param: TsFnParam {
                                name: Some(param_name),
                                ty: TsType { kind: TsTypeKind::Any, span: Span::new(start, end) },
                                optional: false,
                                rest: false,
                            },
                            type_ann: Box::new(type_ann),
                        },
                        span: Span::new(start, end),
                    });
                }

                // Index signature: `[key: Type]: Value`
                let param_optional = self.eat(&TokenKind::Question);
                self.expect(&TokenKind::Colon)?;
                let param_ty = self.parse_ts_type_impl()?;
                self.expect(&TokenKind::RBracket)?;
                // Optional `?` modifier on the value
                self.eat(&TokenKind::Question);
                let type_ann = if self.eat(&TokenKind::Colon) {
                    self.parse_ts_type_impl()?
                } else {
                    // Index signature without type annotation: `[key: Type];`
                    TsType { kind: TsTypeKind::Any, span: Span::new(start, self.current.span.start) }
                };
                let end = self.current.span.start;
                return Ok(TsTypeMember {
                    kind: TsTypeMemberKind::Index {
                        readonly,
                        param: TsFnParam {
                            name: Some(param_name),
                            ty: param_ty,
                            optional: param_optional,
                            rest: false,
                        },
                        type_ann: Box::new(type_ann),
                    },
                    span: Span::new(start, end),
                });
            } else {
                // Computed property key: `[Symbol.dispose]` or `[PROP]`
                let _expr = self.parse_assign_expr()?;
                self.expect(&TokenKind::RBracket)?;
                let key = PropertyKey::Computed(Box::new(_expr));
                let optional = self.eat(&TokenKind::Question);

                // Method signature: `[expr](): Type`
                if self.check(&TokenKind::LParen) || self.check(&TokenKind::Lt) {
                    let type_params = if self.check(&TokenKind::Lt) {
                        Some(self.parse_ts_type_params_impl()?)
                    } else {
                        None
                    };
                    self.expect(&TokenKind::LParen)?;
                    let params = self.parse_ts_fn_params_impl()?;
                    self.expect(&TokenKind::RParen)?;
                    let return_type = if self.eat(&TokenKind::Colon) {
                        Some(Box::new(self.parse_ts_type_impl()?))
                    } else {
                        None
                    };
                    let end = self.current.span.start;
                    return Ok(TsTypeMember {
                        kind: TsTypeMemberKind::Method {
                            key,
                            optional,
                            params,
                            type_params,
                            return_type,
                        },
                        span: Span::new(start, end),
                    });
                }

                // Property signature: `[expr]: Type` or `[expr]?: Type`
                let type_ann = if self.eat(&TokenKind::Colon) {
                    Some(Box::new(self.parse_ts_type_impl()?))
                } else {
                    None
                };
                let end = self.current.span.start;
                return Ok(TsTypeMember {
                    kind: TsTypeMemberKind::Property {
                        key,
                        optional,
                        readonly,
                        type_ann,
                    },
                    span: Span::new(start, end),
                });
            }
        }

        // Optional call signature: `?(): ReturnType`
        // Must check BEFORE the property/method signature path since `?` is not a valid property key
        if self.check(&TokenKind::Question) && matches!(self.lexer.peek().kind, TokenKind::LParen | TokenKind::Lt) {
            self.advance(); // consume `?`
            let type_params = if self.check(&TokenKind::Lt) {
                Some(self.parse_ts_type_params_impl()?)
            } else {
                None
            };
            self.expect(&TokenKind::LParen)?;
            let params = self.parse_ts_fn_params_impl()?;
            self.expect(&TokenKind::RParen)?;
            let return_type = if self.eat(&TokenKind::Colon) {
                Some(Box::new(self.parse_ts_type_impl()?))
            } else {
                None
            };
            let end = self.current.span.start;
            return Ok(TsTypeMember {
                kind: TsTypeMemberKind::CallSignature {
                    params,
                    type_params,
                    return_type,
                },
                span: Span::new(start, end),
            });
        }

        // Call signature: `(params): ReturnType` or `<T>(params): ReturnType` or `()?: ReturnType`
        if self.check(&TokenKind::LParen) || self.check(&TokenKind::Lt) {
            let type_params = if self.check(&TokenKind::Lt) {
                Some(self.parse_ts_type_params_impl()?)
            } else {
                None
            };
            self.expect(&TokenKind::LParen)?;
            let params = self.parse_ts_fn_params_impl()?;
            self.expect(&TokenKind::RParen)?;
            self.eat(&TokenKind::Question); // optional call signature: `()?: any`
            let return_type = if self.eat(&TokenKind::Colon) {
                Some(Box::new(self.parse_ts_type_impl()?))
            } else {
                None
            };
            let end = self.current.span.start;
            return Ok(TsTypeMember {
                kind: TsTypeMemberKind::CallSignature {
                    params,
                    type_params,
                    return_type,
                },
                span: Span::new(start, end),
            });
        }

        // `new` construct signature: `new (params): ReturnType` or `new ?(): ReturnType`
        if self.check(&TokenKind::New) {
            self.advance();
            self.eat(&TokenKind::Question); // optional construct signature
            let type_params = if self.check(&TokenKind::Lt) {
                Some(self.parse_ts_type_params_impl()?)
            } else {
                None
            };
            self.expect(&TokenKind::LParen)?;
            let params = self.parse_ts_fn_params_impl()?;
            self.expect(&TokenKind::RParen)?;
            let return_type = if self.eat(&TokenKind::Colon) {
                Some(Box::new(self.parse_ts_type_impl()?))
            } else {
                None
            };
            let end = self.current.span.start;
            return Ok(TsTypeMember {
                kind: TsTypeMemberKind::ConstructSignature {
                    params,
                    type_params,
                    return_type,
                },
                span: Span::new(start, end),
            });
        }

        // Property or method signature
        let key = self.parse_property_key()?;
        let optional = self.eat(&TokenKind::Question);

        // Method signature: `method(params): ReturnType`
        if self.check(&TokenKind::LParen) || self.check(&TokenKind::Lt) {
            let type_params = if self.check(&TokenKind::Lt) {
                Some(self.parse_ts_type_params_impl()?)
            } else {
                None
            };
            self.expect(&TokenKind::LParen)?;
            let params = self.parse_ts_fn_params_impl()?;
            self.expect(&TokenKind::RParen)?;
            let return_type = if self.eat(&TokenKind::Colon) {
                Some(Box::new(self.parse_ts_type_impl()?))
            } else {
                None
            };
            let end = self.current.span.start;
            return Ok(TsTypeMember {
                kind: TsTypeMemberKind::Method {
                    key,
                    optional,
                    params,
                    type_params,
                    return_type,
                },
                span: Span::new(start, end),
            });
        }

        // Property signature: `prop: Type`
        let type_ann = if self.eat(&TokenKind::Colon) {
            Some(Box::new(self.parse_ts_type_impl()?))
        } else {
            None
        };
        let end = self.current.span.start;

        Ok(TsTypeMember {
            kind: TsTypeMemberKind::Property {
                key,
                optional,
                readonly,
                type_ann,
            },
            span: Span::new(start, end),
        })
    }

    // =========================================================================
    // Type Declarations
    // =========================================================================

    /// Parse a type alias declaration: `type Foo<T> = Bar<T>`
    pub(crate) fn parse_ts_type_alias_impl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Type)?;
        let name = self.expect_ts_identifier()?;

        let type_params = if self.check(&TokenKind::Lt) {
            Some(self.parse_ts_type_params_impl()?)
        } else {
            None
        };

        self.expect(&TokenKind::Eq)?;
        let ty = self.parse_ts_type_impl()?;
        self.expect_semicolon()?;
        let end = self.current.span.start;

        Ok(Stmt::new(
            StmtKind::TsTypeAlias(Box::new(TsTypeAlias {
                name,
                type_params,
                ty,
                span: Span::new(start, end),
            })),
            Span::new(start, end),
        ))
    }

    /// Parse an interface declaration: `interface Foo<T> extends Bar { ... }`
    pub(crate) fn parse_ts_interface_impl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Interface)?;
        let name = self.expect_ts_identifier()?;

        let type_params = if self.check(&TokenKind::Lt) {
            Some(self.parse_ts_type_params_impl()?)
        } else {
            None
        };

        let mut extends = Vec::new();
        if self.eat(&TokenKind::Extends) {
            loop {
                extends.push(self.parse_ts_type_impl()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(&TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            body.push(self.parse_ts_type_member_impl()?);
            if !self.eat(&TokenKind::Semicolon) {
                self.eat(&TokenKind::Comma);
            }
        }
        self.expect(&TokenKind::RBrace)?;
        let end = self.current.span.start;

        Ok(Stmt::new(
            StmtKind::TsInterface(Box::new(TsInterface {
                name,
                type_params,
                extends,
                body,
                span: Span::new(start, end),
            })),
            Span::new(start, end),
        ))
    }

    /// Parse an enum declaration: `enum Foo { A, B = 1 }`
    pub(crate) fn parse_ts_enum_impl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        let is_const = self.eat(&TokenKind::Const);
        self.expect(&TokenKind::Enum)?;
        let name = self.expect_ts_identifier()?;

        self.expect(&TokenKind::LBrace)?;
        let mut members = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            let member_start = self.current.span.start;
            let member_name = match self.peek() {
                TokenKind::String(s) => {
                    let s = s.clone();
                    self.advance();
                    s
                }
                _ => self.expect_ts_identifier()?,
            };
            let init = if self.eat(&TokenKind::Eq) {
                Some(self.parse_assign_expr()?)
            } else {
                None
            };
            let member_end = self.current.span.start;
            members.push(TsEnumMember {
                name: member_name,
                init,
                span: Span::new(member_start, member_end),
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        let end = self.current.span.start;

        Ok(Stmt::new(
            StmtKind::TsEnum(Box::new(TsEnum {
                name,
                is_const,
                members,
                span: Span::new(start, end),
            })),
            Span::new(start, end),
        ))
    }

    /// Parse a namespace/module declaration: `namespace Foo { ... }`
    pub(crate) fn parse_ts_namespace_impl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        // Consume `namespace` or `module`
        self.advance();
        // Module name can be a string literal: `declare module "./path"`
        // or anonymous: `module { ... }`
        let name = if let TokenKind::String(s) = self.peek() {
            let s = s.clone();
            self.advance();
            s
        } else if self.check(&TokenKind::LBrace) {
            // Anonymous module: `module { ... }`
            String::new()
        } else {
            let base = self.expect_ts_identifier()?;
            // Dotted namespace: `namespace A.B.C`
            let mut full = base;
            while self.eat(&TokenKind::Dot) {
                let part = self.expect_ts_identifier()?;
                full.push('.');
                full.push_str(&part);
            }
            full
        };

        self.expect(&TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            body.push(self.parse_stmt()?);
        }
        self.expect(&TokenKind::RBrace)?;
        let end = self.current.span.start;

        Ok(Stmt::new(
            StmtKind::TsNamespace(Box::new(TsNamespace {
                name,
                body,
                span: Span::new(start, end),
            })),
            Span::new(start, end),
        ))
    }

    /// Parse a declare statement: `declare function foo(): void;`
    pub(crate) fn parse_ts_declare_impl(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current.span.start;
        self.expect(&TokenKind::Declare)?;

        // Parse the inner declaration
        // Check for `const enum` before the main match to avoid borrow conflicts
        let is_const_enum = matches!(self.peek(), TokenKind::Const)
            && matches!(self.lexer.peek().kind, TokenKind::Enum);
        let is_global = matches!(self.peek(), TokenKind::Identifier(ref n) if n == "global");

        let inner = if is_const_enum {
            self.advance(); // consume `const`
            self.parse_ts_enum_impl()?
        } else if is_global {
            self.advance(); // consume `global`
            self.expect(&TokenKind::LBrace)?;
            let mut body = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                body.push(self.parse_stmt()?);
            }
            self.expect(&TokenKind::RBrace)?;
            let end = self.current.span.start;
            Stmt::new(StmtKind::Empty, Span::new(start, end))
        } else {
            match self.peek() {
                TokenKind::Function => self.parse_function_decl()?,
                TokenKind::Class => self.parse_class_decl()?,
                TokenKind::Var | TokenKind::Let | TokenKind::Const => self.parse_var_decl()?,
                TokenKind::Enum => self.parse_ts_enum_impl()?,
                TokenKind::Namespace | TokenKind::Module => self.parse_ts_namespace_impl()?,
                TokenKind::Interface => self.parse_ts_interface_impl()?,
                TokenKind::Type => self.parse_ts_type_alias_impl()?,
                TokenKind::Abstract => {
                    // declare abstract class ...
                    self.advance(); // consume abstract
                    self.parse_class_decl()?
                }
                TokenKind::Export => {
                    // declare export ...
                    self.parse_export_decl()?
                }
                _ => {
                    return Err(ParseError::new(
                        format!("Expected declaration after 'declare', got {:?}", self.peek()),
                        self.current.span,
                    ));
                }
            }
        };

        let end = self.current.span.start;
        Ok(Stmt::new(
            StmtKind::TsDeclare(Box::new(inner)),
            Span::new(start, end),
        ))
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    /// Parse the operand for `typeof` in type position: a dotted name like `Foo.bar.baz`.
    /// Does NOT consume `[...]` since that would be an indexed access type, not member access.
    fn parse_ts_typeof_operand(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;
        let name = self.expect_ts_identifier()?;
        let mut expr = Expr::new(ExprKind::Ident(name), Span::new(start, self.current.span.start));
        // Parse dotted access: Foo.bar.baz
        while self.eat(&TokenKind::Dot) {
            let prop = self.expect_ts_identifier()?;
            let end = self.current.span.start;
            expr = Expr::new(
                ExprKind::Member {
                    object: Box::new(expr),
                    property: Box::new(Expr::new(ExprKind::Ident(prop), Span::new(start, end))),
                    computed: false,
                },
                Span::new(start, end),
            );
        }
        // Consume optional type args: typeof fn<number>
        if self.check(&TokenKind::Lt) {
            let _ = self.parse_ts_type_args_impl()?;
        }
        Ok(expr)
    }

    /// Accept an identifier, including TS contextual keywords used as identifiers.
    pub(crate) fn expect_ts_identifier(&mut self) -> Result<String, ParseError> {
        match self.peek().clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            // TS contextual keywords can be used as identifiers in many positions
            TokenKind::Type => { self.advance(); Ok("type".to_string()) }
            TokenKind::Interface => { self.advance(); Ok("interface".to_string()) }
            TokenKind::Namespace => { self.advance(); Ok("namespace".to_string()) }
            TokenKind::Module => { self.advance(); Ok("module".to_string()) }
            TokenKind::Declare => { self.advance(); Ok("declare".to_string()) }
            TokenKind::Abstract => { self.advance(); Ok("abstract".to_string()) }
            TokenKind::Readonly => { self.advance(); Ok("readonly".to_string()) }
            TokenKind::Override => { self.advance(); Ok("override".to_string()) }
            TokenKind::Any => { self.advance(); Ok("any".to_string()) }
            TokenKind::Unknown => { self.advance(); Ok("unknown".to_string()) }
            TokenKind::Never => { self.advance(); Ok("never".to_string()) }
            TokenKind::Is => { self.advance(); Ok("is".to_string()) }
            TokenKind::Asserts => { self.advance(); Ok("asserts".to_string()) }
            TokenKind::Satisfies => { self.advance(); Ok("satisfies".to_string()) }
            TokenKind::Keyof => { self.advance(); Ok("keyof".to_string()) }
            TokenKind::Infer => { self.advance(); Ok("infer".to_string()) }
            TokenKind::Enum => { self.advance(); Ok("enum".to_string()) }
            // Other keywords that can be used as property names
            TokenKind::Async => { self.advance(); Ok("async".to_string()) }
            TokenKind::From => { self.advance(); Ok("from".to_string()) }
            TokenKind::Get => { self.advance(); Ok("get".to_string()) }
            TokenKind::Set => { self.advance(); Ok("set".to_string()) }
            TokenKind::As => { self.advance(); Ok("as".to_string()) }
            TokenKind::Static => { self.advance(); Ok("static".to_string()) }
            // Fall through to JS keywords (default, let, new, delete, etc.)
            _ => {
                let name = crate::parser::keyword_to_str(self.peek());
                if !name.is_empty() {
                    let s = name.to_string();
                    self.advance();
                    Ok(s)
                } else {
                    Err(ParseError::new(
                        format!("Expected identifier, got {:?}", self.peek()),
                        self.current.span,
                    ))
                }
            }
        }
    }

    /// Try to consume TypeScript accessibility modifier (public/private/protected).
    /// Returns the accessibility if found.
    /// Check if the current token (a TS modifier keyword) is actually being used as a
    /// property name rather than a modifier. True when the next token is `:`, `=`, `;`,
    /// `?`, `!`, or `(` — all of which indicate the keyword IS the property/method name.
    pub(crate) fn is_ts_modifier_used_as_property(&mut self) -> bool {
        matches!(self.lexer.peek().kind,
            TokenKind::Colon | TokenKind::Eq | TokenKind::Semicolon
            | TokenKind::Question | TokenKind::Bang | TokenKind::LParen
        ) || self.lexer.peek().had_newline_before
    }

    pub(crate) fn try_parse_accessibility(&mut self) -> Option<Accessibility> {
        let is_access = matches!(self.peek(), TokenKind::Public | TokenKind::Protected | TokenKind::Private);
        if !is_access || self.is_ts_modifier_used_as_property() {
            return None;
        }
        match self.peek() {
            TokenKind::Public => { self.advance(); Some(Accessibility::Public) }
            TokenKind::Protected => { self.advance(); Some(Accessibility::Protected) }
            TokenKind::Private => { self.advance(); Some(Accessibility::Private) }
            _ => None,
        }
    }

    /// Try to parse type arguments `<T, U>` before a call expression.
    /// Returns true (and consumes the type args) if successful and followed by `(`.
    /// Returns false (and restores parser state) otherwise.
    /// Parse `<T>expr` type assertion or `<T>(x) => body` generic arrow function.
    /// Called from parse_primary_expr when `<` is current token in .ts (not .tsx) files.
    pub(crate) fn parse_ts_angle_bracket_expr(&mut self, start: u32) -> Result<Expr, ParseError> {
        // Save state to try generic arrow first
        let saved_current = self.current.clone();
        let saved_lexer = self.lexer.clone();

        // Try as generic arrow: <T>(params) => body
        if let Ok(type_params) = self.parse_ts_type_params_impl() {
            if self.check(&TokenKind::LParen) {
                // Parse params
                self.expect(&TokenKind::LParen)?;
                let params = self.parse_params_inner()?;
                self.expect(&TokenKind::RParen)?;
                // Optional return type
                if self.eat(&TokenKind::Colon) {
                    let _ = self.parse_ts_type()?;
                }
                if self.check(&TokenKind::Arrow) {
                    self.advance();
                    let _ = type_params; // type params stripped in codegen
                    return self.parse_arrow_body(params, false, start);
                }
            }
        }

        // Restore state — parse as type assertion: <Type>expr
        self.current = saved_current;
        self.lexer = saved_lexer;

        self.expect(&TokenKind::Lt)?;
        let _ = self.parse_ts_type_impl()?;
        self.expect_ts_gt()?;
        // Parse the expression being asserted
        let expr = self.parse_unary_expr()?;
        let end = self.current.span.start;
        // Return the inner expression (type assertion stripped)
        Ok(Expr::new(expr.kind, Span::new(start, end)))
    }

    pub(crate) fn try_parse_ts_type_args_for_call(&mut self) -> bool {
        if !self.check(&TokenKind::Lt) {
            return false;
        }
        // Save state for backtracking
        let saved_current = self.current.clone();
        let saved_lexer = self.lexer.clone();

        match self.parse_ts_type_args_impl() {
            Ok(_) => {
                // Type args parsed successfully; check follow token
                // Valid after type args: call `(`, tagged template, or instantiation expression
                // Instantiation expression follow tokens: `;`, `,`, `)`, `]`, `.`, `?.`, `!`, EOF
                if self.check(&TokenKind::LParen)
                    || matches!(self.peek(), TokenKind::TemplateNoSub(_) | TokenKind::TemplateHead(_))
                    || matches!(self.peek(),
                        TokenKind::Semicolon | TokenKind::Comma | TokenKind::RParen
                        | TokenKind::RBracket | TokenKind::Dot | TokenKind::QuestionDot
                        | TokenKind::Bang | TokenKind::Eof
                    )
                {
                    true // keep consumed type args
                } else {
                    // Not a call or instantiation — restore state
                    self.current = saved_current;
                    self.lexer = saved_lexer;
                    false
                }
            }
            Err(_) => {
                // Parse failed — restore state
                self.current = saved_current;
                self.lexer = saved_lexer;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserOptions;

    fn parse_ts(source: &str) -> Result<crate::ast::Ast, ParseError> {
        Parser::new(source, ParserOptions {
            module: true,
            typescript: true,
            #[cfg(feature = "jsx")]
            jsx: false,
        }).parse()
    }

    // =========================================================================
    // Type Aliases
    // =========================================================================

    #[test]
    fn test_type_alias_simple() {
        let ast = parse_ts("type Foo = string;").unwrap();
        assert_eq!(ast.stmts.len(), 1);
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                assert_eq!(alias.name, "Foo");
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_type_alias_generic() {
        let ast = parse_ts("type Result<T, E> = T | E;").unwrap();
        assert_eq!(ast.stmts.len(), 1);
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                assert_eq!(alias.name, "Result");
                assert!(alias.type_params.is_some());
                assert_eq!(alias.type_params.as_ref().unwrap().len(), 2);
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_type_alias_with_constraint_and_default() {
        parse_ts("type Foo<T extends string = 'hello'> = T;").unwrap();
    }

    // =========================================================================
    // Keyword Types
    // =========================================================================

    #[test]
    fn test_keyword_types() {
        for kw in &["any", "unknown", "never", "void", "null", "undefined",
                     "number", "string", "boolean", "symbol", "bigint", "object"] {
            parse_ts(&format!("type X = {};", kw)).unwrap();
        }
    }

    // =========================================================================
    // Literal Types
    // =========================================================================

    #[test]
    fn test_literal_types() {
        let ast = parse_ts("type Foo = \"hello\" | 42 | true;").unwrap();
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                match &alias.ty.kind {
                    TsTypeKind::Union(types) => {
                        assert_eq!(types.len(), 3);
                        assert!(matches!(&types[0].kind, TsTypeKind::LitString(_)));
                        assert!(matches!(&types[1].kind, TsTypeKind::LitNumber(_)));
                        assert!(matches!(&types[2].kind, TsTypeKind::LitBoolean(true)));
                    }
                    _ => panic!("Expected Union type"),
                }
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_negative_literal_type() {
        parse_ts("type X = -1;").unwrap();
    }

    #[test]
    fn test_false_literal_type() {
        parse_ts("type X = false;").unwrap();
    }

    // =========================================================================
    // Union and Intersection Types
    // =========================================================================

    #[test]
    fn test_union_type() {
        let ast = parse_ts("type Foo = string | number | boolean;").unwrap();
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                match &alias.ty.kind {
                    TsTypeKind::Union(types) => assert_eq!(types.len(), 3),
                    _ => panic!("Expected Union"),
                }
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_intersection_type() {
        let ast = parse_ts("type Foo = A & B & C;").unwrap();
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                match &alias.ty.kind {
                    TsTypeKind::Intersection(types) => assert_eq!(types.len(), 3),
                    _ => panic!("Expected Intersection"),
                }
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_leading_union_bar() {
        parse_ts("type X = | 'a' | 'b' | 'c';").unwrap();
    }

    #[test]
    fn test_leading_intersection_ampersand() {
        parse_ts("type X = & A & B;").unwrap();
    }

    #[test]
    fn test_union_intersection_mixed() {
        parse_ts("type X = (A & B) | (C & D);").unwrap();
    }

    // =========================================================================
    // Array Types
    // =========================================================================

    #[test]
    fn test_array_type() {
        let ast = parse_ts("type Foo = number[];").unwrap();
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                match &alias.ty.kind {
                    TsTypeKind::Array(_) => {}
                    _ => panic!("Expected Array type"),
                }
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_nested_array_type() {
        parse_ts("type X = string[][];").unwrap();
    }

    #[test]
    fn test_readonly_array_type() {
        parse_ts("type X = readonly string[];").unwrap();
    }

    // =========================================================================
    // Tuple Types
    // =========================================================================

    #[test]
    fn test_tuple_type() {
        let ast = parse_ts("type Foo = [string, number, boolean];").unwrap();
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                match &alias.ty.kind {
                    TsTypeKind::Tuple(types) => assert_eq!(types.len(), 3),
                    _ => panic!("Expected Tuple type"),
                }
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_labeled_tuple() {
        parse_ts("type X = [name: string, age: number];").unwrap();
    }

    #[test]
    fn test_optional_tuple_element() {
        parse_ts("type X = [string, number?];").unwrap();
    }

    #[test]
    fn test_rest_tuple_element() {
        parse_ts("type X = [string, ...number[]];").unwrap();
    }

    #[test]
    fn test_empty_tuple() {
        parse_ts("type X = [];").unwrap();
    }

    // =========================================================================
    // Type References and Generics
    // =========================================================================

    #[test]
    fn test_type_reference_with_args() {
        let ast = parse_ts("type Foo = Promise<string>;").unwrap();
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                match &alias.ty.kind {
                    TsTypeKind::Reference { name, type_args } => {
                        assert_eq!(name, "Promise");
                        assert!(type_args.is_some());
                    }
                    _ => panic!("Expected Reference type"),
                }
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_nested_generics() {
        parse_ts("type X = Map<string, Array<number>>;").unwrap();
    }

    #[test]
    fn test_qualified_type_reference() {
        parse_ts("type X = Foo.Bar.Baz;").unwrap();
    }

    #[test]
    fn test_qualified_type_with_generics() {
        parse_ts("type X = Foo.Bar<string>;").unwrap();
    }

    // =========================================================================
    // Function Types
    // =========================================================================

    #[test]
    fn test_function_type() {
        parse_ts("type F = (x: number, y: string) => boolean;").unwrap();
    }

    #[test]
    fn test_function_type_no_params() {
        parse_ts("type F = () => void;").unwrap();
    }

    #[test]
    fn test_function_type_generic() {
        parse_ts("type F = <T>(x: T) => T;").unwrap();
    }

    #[test]
    fn test_function_type_rest_param() {
        parse_ts("type F = (...args: string[]) => void;").unwrap();
    }

    #[test]
    fn test_function_type_optional_param() {
        parse_ts("type F = (x?: number) => void;").unwrap();
    }

    #[test]
    fn test_construct_signature_type() {
        parse_ts("type C = new (x: string) => Foo;").unwrap();
    }

    // =========================================================================
    // Object Type Literals
    // =========================================================================

    #[test]
    fn test_object_type_literal() {
        let ast = parse_ts("type Obj = { x: number; y: string };").unwrap();
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                match &alias.ty.kind {
                    TsTypeKind::TypeLiteral(members) => assert_eq!(members.len(), 2),
                    _ => panic!("Expected TypeLiteral"),
                }
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_object_type_optional_property() {
        parse_ts("type X = { x?: number };").unwrap();
    }

    #[test]
    fn test_object_type_readonly_property() {
        parse_ts("type X = { readonly x: number };").unwrap();
    }

    #[test]
    fn test_object_type_method_signature() {
        parse_ts("type X = { foo(x: number): string };").unwrap();
    }

    #[test]
    fn test_object_type_call_signature() {
        parse_ts("type X = { (x: number): string };").unwrap();
    }

    #[test]
    fn test_object_type_construct_signature() {
        parse_ts("type X = { new (x: string): Foo };").unwrap();
    }

    #[test]
    fn test_object_type_index_signature() {
        parse_ts("type X = { [key: string]: number };").unwrap();
    }

    #[test]
    fn test_object_type_computed_property() {
        parse_ts("type X = { [Symbol.iterator](): Iterator<any> };").unwrap();
    }

    // =========================================================================
    // Conditional Types
    // =========================================================================

    #[test]
    fn test_conditional_type() {
        let ast = parse_ts("type IsString<T> = T extends string ? true : false;").unwrap();
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                match &alias.ty.kind {
                    TsTypeKind::Conditional { .. } => {}
                    _ => panic!("Expected Conditional type"),
                }
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_nested_conditional_type() {
        parse_ts("type X<T> = T extends string ? 'str' : T extends number ? 'num' : 'other';").unwrap();
    }

    #[test]
    fn test_conditional_with_infer() {
        parse_ts("type Unpacked<T> = T extends Array<infer U> ? U : T;").unwrap();
    }

    #[test]
    fn test_conditional_with_infer_constraint() {
        parse_ts("type X<T> = T extends { a: infer U extends string } ? U : never;").unwrap();
    }

    // =========================================================================
    // Mapped Types
    // =========================================================================

    #[test]
    fn test_mapped_type() {
        parse_ts("type Readonly<T> = { readonly [K in keyof T]: T[K] };").unwrap();
    }

    #[test]
    fn test_mapped_type_optional() {
        parse_ts("type Partial<T> = { [K in keyof T]?: T[K] };").unwrap();
    }

    #[test]
    fn test_mapped_type_remove_readonly() {
        parse_ts("type Mutable<T> = { -readonly [K in keyof T]: T[K] };").unwrap();
    }

    #[test]
    fn test_mapped_type_as_clause() {
        parse_ts("type Getters<T> = { [K in keyof T as `get${string}`]: () => T[K] };").unwrap();
    }

    // =========================================================================
    // Template Literal Types
    // =========================================================================

    #[test]
    fn test_template_literal_type() {
        parse_ts("type X = `hello ${string}`;").unwrap();
    }

    #[test]
    fn test_template_literal_type_union() {
        parse_ts("type Event = `${'click' | 'focus'}_handler`;").unwrap();
    }

    // =========================================================================
    // Indexed Access and Keyof/Typeof
    // =========================================================================

    #[test]
    fn test_indexed_access_type() {
        parse_ts("type X = Foo[\"key\"];").unwrap();
    }

    #[test]
    fn test_indexed_access_number() {
        parse_ts("type X = Foo[number];").unwrap();
    }

    #[test]
    fn test_chained_indexed_access() {
        parse_ts("type X = Foo[\"key\"][\"nested\"];").unwrap();
    }

    #[test]
    fn test_keyof_type() {
        let ast = parse_ts("type Keys = keyof Foo;").unwrap();
        match &ast.stmts[0].kind {
            StmtKind::TsTypeAlias(alias) => {
                match &alias.ty.kind {
                    TsTypeKind::Keyof(_) => {}
                    _ => panic!("Expected Keyof type"),
                }
            }
            _ => panic!("Expected TsTypeAlias"),
        }
    }

    #[test]
    fn test_typeof_type() {
        parse_ts("type X = typeof myVar;").unwrap();
    }

    #[test]
    fn test_typeof_qualified() {
        parse_ts("type X = typeof obj.prop;").unwrap();
    }

    #[test]
    fn test_typeof_import() {
        parse_ts("type X = typeof import('./mod');").unwrap();
    }

    // =========================================================================
    // Import Types
    // =========================================================================

    #[test]
    fn test_import_type() {
        parse_ts("type X = import('./module').Foo;").unwrap();
    }

    #[test]
    fn test_import_type_with_generics() {
        parse_ts("type X = import('./module').Foo<string>;").unwrap();
    }

    // =========================================================================
    // Parenthesized and This Types
    // =========================================================================

    #[test]
    fn test_parenthesized_type() {
        parse_ts("type X = (A | B)[];").unwrap();
    }

    #[test]
    fn test_this_type() {
        parse_ts("type X = { clone(): this };").unwrap();
    }

    // =========================================================================
    // Interfaces
    // =========================================================================

    #[test]
    fn test_interface() {
        let ast = parse_ts("interface Foo { x: number; y: string }").unwrap();
        assert_eq!(ast.stmts.len(), 1);
        match &ast.stmts[0].kind {
            StmtKind::TsInterface(iface) => {
                assert_eq!(iface.name, "Foo");
                assert_eq!(iface.body.len(), 2);
            }
            _ => panic!("Expected TsInterface"),
        }
    }

    #[test]
    fn test_interface_extends() {
        let ast = parse_ts("interface Bar extends Foo { z: boolean }").unwrap();
        assert_eq!(ast.stmts.len(), 1);
        match &ast.stmts[0].kind {
            StmtKind::TsInterface(iface) => {
                assert_eq!(iface.name, "Bar");
                assert_eq!(iface.extends.len(), 1);
            }
            _ => panic!("Expected TsInterface"),
        }
    }

    #[test]
    fn test_interface_extends_multiple() {
        parse_ts("interface Foo extends A, B, C { x: number }").unwrap();
    }

    #[test]
    fn test_interface_generic() {
        parse_ts("interface Box<T> { value: T }").unwrap();
    }

    #[test]
    fn test_interface_with_call_signature() {
        parse_ts("interface Callable { (x: number): string }").unwrap();
    }

    #[test]
    fn test_interface_with_construct_signature() {
        parse_ts("interface Newable { new (x: string): Foo }").unwrap();
    }

    #[test]
    fn test_interface_with_index_signature() {
        parse_ts("interface Dict { [key: string]: any }").unwrap();
    }

    #[test]
    fn test_interface_with_optional_method() {
        parse_ts("interface Foo { bar?(x: number): void }").unwrap();
    }

    #[test]
    fn test_interface_with_computed_key() {
        parse_ts("interface Foo { [Symbol.iterator]?: never }").unwrap();
    }

    #[test]
    fn test_interface_optional_call_signature() {
        parse_ts("interface Foo { ?(): any }").unwrap();
    }

    #[test]
    fn test_interface_optional_construct_signature() {
        parse_ts("interface Foo { new ?(): any }").unwrap();
    }

    // =========================================================================
    // Enums
    // =========================================================================

    #[test]
    fn test_enum() {
        let ast = parse_ts("enum Direction { Up, Down, Left, Right }").unwrap();
        assert_eq!(ast.stmts.len(), 1);
        match &ast.stmts[0].kind {
            StmtKind::TsEnum(en) => {
                assert_eq!(en.name, "Direction");
                assert_eq!(en.members.len(), 4);
                assert!(!en.is_const);
            }
            _ => panic!("Expected TsEnum"),
        }
    }

    #[test]
    fn test_const_enum() {
        let ast = parse_ts("const enum Color { Red = 0, Green = 1, Blue = 2 }").unwrap();
        assert_eq!(ast.stmts.len(), 1);
        match &ast.stmts[0].kind {
            StmtKind::TsEnum(en) => {
                assert_eq!(en.name, "Color");
                assert!(en.is_const);
                assert_eq!(en.members.len(), 3);
            }
            _ => panic!("Expected TsEnum"),
        }
    }

    #[test]
    fn test_enum_string_values() {
        parse_ts("enum Dir { Up = 'UP', Down = 'DOWN' }").unwrap();
    }

    #[test]
    fn test_enum_computed_values() {
        parse_ts("enum E { A = 1 + 2, B = A * 3 }").unwrap();
    }

    #[test]
    fn test_enum_trailing_comma() {
        parse_ts("enum E { A, B, C, }").unwrap();
    }

    // =========================================================================
    // Namespaces
    // =========================================================================

    #[test]
    fn test_namespace() {
        let ast = parse_ts("namespace Foo { export const x = 1; }").unwrap();
        assert_eq!(ast.stmts.len(), 1);
        match &ast.stmts[0].kind {
            StmtKind::TsNamespace(ns) => {
                assert_eq!(ns.name, "Foo");
                assert_eq!(ns.body.len(), 1);
            }
            _ => panic!("Expected TsNamespace"),
        }
    }

    #[test]
    fn test_module_keyword() {
        parse_ts("module Foo { export const x = 1; }").unwrap();
    }

    #[test]
    fn test_nested_namespace() {
        parse_ts("namespace Outer { namespace Inner { export const x = 1; } }").unwrap();
    }

    #[test]
    fn test_anonymous_module() {
        parse_ts("module { const x = 1; }").unwrap();
    }

    // =========================================================================
    // Declare
    // =========================================================================

    #[test]
    fn test_declare_function() {
        let ast = parse_ts("declare function foo(): void;").unwrap();
        assert_eq!(ast.stmts.len(), 1);
        match &ast.stmts[0].kind {
            StmtKind::TsDeclare(_) => {}
            _ => panic!("Expected TsDeclare"),
        }
    }

    #[test]
    fn test_declare_var() {
        parse_ts("declare var x: number;").unwrap();
    }

    #[test]
    fn test_declare_let() {
        parse_ts("declare let x: string;").unwrap();
    }

    #[test]
    fn test_declare_const() {
        parse_ts("declare const x: boolean;").unwrap();
    }

    #[test]
    fn test_declare_class() {
        parse_ts("declare class Foo { bar(): void }").unwrap();
    }

    #[test]
    fn test_declare_interface() {
        parse_ts("declare interface Foo { x: number }").unwrap();
    }

    #[test]
    fn test_declare_enum() {
        parse_ts("declare enum Direction { Up, Down }").unwrap();
    }

    #[test]
    fn test_declare_const_enum() {
        parse_ts("declare const enum Color { Red, Green }").unwrap();
    }

    #[test]
    fn test_declare_namespace() {
        parse_ts("declare namespace Foo { function bar(): void }").unwrap();
    }

    #[test]
    fn test_declare_module() {
        parse_ts("declare module 'foo' { export const x: number; }").unwrap();
    }

    #[test]
    fn test_declare_global() {
        parse_ts("declare global { interface Window { foo: string } }").unwrap();
    }

    #[test]
    fn test_declare_type() {
        parse_ts("declare type Foo = string;").unwrap();
    }

    // =========================================================================
    // Variable and Function Type Annotations
    // =========================================================================

    #[test]
    fn test_var_with_type_annotation() {
        let ast = parse_ts("let x: number = 5;").unwrap();
        assert_eq!(ast.stmts.len(), 1);
    }

    #[test]
    fn test_const_with_type_annotation() {
        parse_ts("const name: string = 'hello';").unwrap();
    }

    #[test]
    fn test_function_param_types() {
        parse_ts("function add(a: number, b: number): number { return a + b; }").unwrap();
    }

    #[test]
    fn test_function_optional_param() {
        parse_ts("function foo(x: number, y?: string): void {}").unwrap();
    }

    #[test]
    fn test_function_rest_param_typed() {
        parse_ts("function foo(...args: string[]): void {}").unwrap();
    }

    #[test]
    fn test_function_generic() {
        parse_ts("function identity<T>(x: T): T { return x; }").unwrap();
    }

    #[test]
    fn test_function_multiple_type_params() {
        parse_ts("function map<T, U>(arr: T[], fn: (x: T) => U): U[] { return arr.map(fn); }").unwrap();
    }

    #[test]
    fn test_function_type_param_constraint() {
        parse_ts("function foo<T extends string>(x: T): T { return x; }").unwrap();
    }

    #[test]
    fn test_function_type_param_default() {
        parse_ts("function foo<T = string>(x: T): T { return x; }").unwrap();
    }

    // =========================================================================
    // Arrow Functions with Types
    // =========================================================================

    #[test]
    fn test_arrow_with_return_type() {
        parse_ts("const add = (a: number, b: number): number => a + b;").unwrap();
    }

    #[test]
    fn test_generic_arrow() {
        parse_ts("const identity = <T>(x: T): T => x;").unwrap();
    }

    #[test]
    fn test_arrow_single_param_typed() {
        parse_ts("const fn = (x: string): void => {};").unwrap();
    }

    // =========================================================================
    // Class TypeScript Features
    // =========================================================================

    #[test]
    fn test_class_implements() {
        parse_ts("class Foo implements Bar, Baz {}").unwrap();
    }

    #[test]
    fn test_class_generic() {
        parse_ts("class Box<T> { value: T; constructor(v: T) { this.value = v; } }").unwrap();
    }

    #[test]
    fn test_class_access_modifiers() {
        parse_ts("class Foo { public x: number; private y: string; protected z: boolean; }").unwrap();
    }

    #[test]
    fn test_class_readonly_property() {
        parse_ts("class Foo { readonly name: string; }").unwrap();
    }

    #[test]
    fn test_class_abstract() {
        parse_ts("abstract class Foo { abstract bar(): void; }").unwrap();
    }

    #[test]
    fn test_class_override() {
        parse_ts("class B extends A { override foo(): void {} }").unwrap();
    }

    #[test]
    fn test_class_optional_property() {
        parse_ts("class Foo { x?: number; }").unwrap();
    }

    #[test]
    fn test_class_definite_assignment() {
        parse_ts("class Foo { x!: number; }").unwrap();
    }

    #[test]
    fn test_class_accessor_keyword() {
        parse_ts("class Foo { accessor name: string = ''; }").unwrap();
    }

    #[test]
    fn test_class_index_signature() {
        parse_ts("class Foo { [key: string]: any }").unwrap();
    }

    #[test]
    fn test_class_generic_method() {
        parse_ts("class Foo { bar<T>(x: T): T { return x; } }").unwrap();
    }

    #[test]
    fn test_class_generic_set_method() {
        parse_ts("class Foo { set<K>(key: string, value: K): void {} }").unwrap();
    }

    #[test]
    fn test_class_constructor_param_modifiers() {
        parse_ts("class Foo { constructor(public x: number, private y: string) {} }").unwrap();
    }

    // =========================================================================
    // Expression-Level TypeScript
    // =========================================================================

    #[test]
    fn test_as_expression() {
        parse_ts("let x = foo as string;").unwrap();
    }

    #[test]
    fn test_as_const() {
        parse_ts("const x = [1, 2, 3] as const;").unwrap();
    }

    #[test]
    fn test_satisfies_expression() {
        parse_ts("const x = { a: 1 } satisfies Record<string, number>;").unwrap();
    }

    #[test]
    fn test_non_null_assertion() {
        parse_ts("let x = foo!;").unwrap();
    }

    #[test]
    fn test_non_null_chained() {
        parse_ts("let x = foo!.bar!.baz;").unwrap();
    }

    #[test]
    fn test_type_assertion_angle_bracket() {
        parse_ts("let x = <string>foo;").unwrap();
    }

    #[test]
    fn test_instantiation_expression() {
        parse_ts("const numId = identity<number>;").unwrap();
    }

    #[test]
    fn test_generic_call() {
        parse_ts("let x = foo<string>(bar);").unwrap();
    }

    #[test]
    fn test_generic_call_multiple_args() {
        parse_ts("let x = foo<string, number>(a, b);").unwrap();
    }

    // =========================================================================
    // Import/Export Types
    // =========================================================================

    #[test]
    fn test_import_type_only() {
        parse_ts("import type { Foo } from './foo';").unwrap();
    }

    #[test]
    fn test_import_inline_type() {
        parse_ts("import { type Foo, bar } from './mod';").unwrap();
    }

    #[test]
    fn test_import_type_default() {
        parse_ts("import type Foo from './foo';").unwrap();
    }

    #[test]
    fn test_import_type_namespace() {
        parse_ts("import type * as Foo from './foo';").unwrap();
    }

    #[test]
    fn test_export_type_only() {
        parse_ts("export type { Foo } from './foo';").unwrap();
    }

    #[test]
    fn test_export_type_named() {
        parse_ts("export type { Foo, Bar };").unwrap();
    }

    #[test]
    fn test_export_inline_type() {
        parse_ts("export { type Foo, bar } from './mod';").unwrap();
    }

    // =========================================================================
    // Disambiguation Edge Cases
    // =========================================================================

    #[test]
    fn test_arrow_vs_ternary_disambiguation() {
        // Should parse as ternary, not arrow
        parse_ts("const x = cond ? (a) : b;").unwrap();
    }

    #[test]
    fn test_arrow_with_return_type_disambiguation() {
        // Should parse as arrow with return type
        parse_ts("const fn = (x: number): string => x.toString();").unwrap();
    }

    #[test]
    fn test_type_postfix_asi() {
        // [Symbol.iterator] on a new line should NOT be indexed access on the type
        parse_ts("interface Foo {\n  x: number;\n  [Symbol.iterator]?: never\n}").unwrap();
    }

    #[test]
    fn test_generic_arrow_vs_comparison() {
        // <T> followed by ( should be generic arrow, not comparison
        parse_ts("const f = <T>(x: T) => x;").unwrap();
    }

    #[test]
    fn test_angle_bracket_generic_call_vs_comparison() {
        // foo<T>() is a generic call
        parse_ts("foo<number>();").unwrap();
    }

    // =========================================================================
    // Complex / Real-World Patterns
    // =========================================================================

    #[test]
    fn test_vue_component_pattern() {
        parse_ts(r#"
import type { VNode } from './vnode'
import {
  type ComponentInternalInstance,
  type ConcreteComponent,
  type Data,
  formatComponentName,
} from './component'
import { isFunction, isString } from '@vue/shared'

type ComponentVNode = VNode & {
  type: ConcreteComponent
}
"#).unwrap();
    }

    #[test]
    fn test_utility_types() {
        parse_ts(r#"
type Partial<T> = { [P in keyof T]?: T[P] };
type Required<T> = { [P in keyof T]-?: T[P] };
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Record<K extends string, V> = { [P in K]: V };
type Exclude<T, U> = T extends U ? never : T;
type Extract<T, U> = T extends U ? T : never;
type NonNullable<T> = T extends null | undefined ? never : T;
type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never;
type Parameters<T> = T extends (...args: infer P) => any ? P : never;
type InstanceType<T> = T extends new (...args: any[]) => infer R ? R : never;
"#).unwrap();
    }

    #[test]
    fn test_class_with_all_features() {
        parse_ts(r#"
abstract class Base<T extends object> implements Iterable<T> {
  public readonly id: string;
  private _value!: T;
  protected items?: T[];
  static count: number = 0;
  accessor name: string = '';

  constructor(public config: T) {
    this.id = 'test';
  }

  abstract process(input: T): Promise<T>;

  get value(): T { return this._value; }
  set value(v: T) { this._value = v; }

  *[Symbol.iterator](): Iterator<T> {
    if (this.items) yield* this.items;
  }

  async map<U>(fn: (item: T) => U): Promise<U[]> {
    return [];
  }
}
"#).unwrap();
    }

    #[test]
    fn test_complex_generics() {
        parse_ts(r#"
type DeepReadonly<T> = T extends Primitive
  ? T
  : T extends Array<infer U>
  ? ReadonlyArray<DeepReadonly<U>>
  : T extends Map<infer K, infer V>
  ? ReadonlyMap<DeepReadonly<K>, DeepReadonly<V>>
  : { readonly [K in keyof T]: DeepReadonly<T[K]> };
"#).unwrap();
    }

    #[test]
    fn test_overloaded_function() {
        parse_ts(r#"
function create(x: string): string;
function create(x: number): number;
function create(x: string | number): string | number {
  return x;
}
"#).unwrap();
    }

    #[test]
    fn test_global_augmentation() {
        parse_ts("global { interface Window { foo: string } }").unwrap();
    }

    #[test]
    fn test_index_signature_without_value_type() {
        parse_ts("interface Foo { [key: string]; }").unwrap();
    }

    #[test]
    fn test_mixed_runtime_and_types() {
        parse_ts(r#"
interface Logger { log(msg: string): void; }
type Level = 'info' | 'warn' | 'error';
declare const globalLogger: Logger;

function createLogger(level: Level): Logger {
  return {
    log(msg: string): void {
      console.log(`[${level}] ${msg}`);
    }
  };
}

const logger: Logger = createLogger('info');
export { logger, createLogger };
export type { Logger, Level };
"#).unwrap();
    }

    #[test]
    fn test_enum_member_expression_initializer() {
        parse_ts(r#"
enum FileAccess {
  None,
  Read = 1 << 1,
  Write = 1 << 2,
  ReadWrite = Read | Write,
}
"#).unwrap();
    }

    #[test]
    fn test_declaration_file_patterns() {
        parse_ts(r#"
declare module 'my-lib' {
  export interface Config {
    debug?: boolean;
    target: string;
  }
  export function init(config: Config): void;
  export class Client {
    constructor(config: Config);
    send(data: unknown): Promise<void>;
  }
  export const VERSION: string;
}
"#).unwrap();
    }
}
