//! JSX-specific parsing and code generation.
//!
//! This module contains:
//! - Context-sensitive detection for JSX vs comparison operators
//! - JSX code generation (JSX → _jsx/_jsxs calls)
//! - JSX parsing methods (integrated into the Parser via extension trait)

use crate::ast::*;
use crate::parser::{ParseError, Parser};
use crate::span::Span;
use crate::token::TokenKind;

/// Check if currently in a JSX context where `<` should be parsed as JSX.
/// This is context-sensitive: `<` after certain tokens is comparison.
pub fn should_parse_jsx(prev_token: Option<&TokenKind>) -> bool {
    match prev_token {
        // After these, `<` is definitely JSX
        Some(TokenKind::LParen)
        | Some(TokenKind::LBrace)
        | Some(TokenKind::LBracket)
        | Some(TokenKind::Comma)
        | Some(TokenKind::Colon)
        | Some(TokenKind::Semicolon)
        | Some(TokenKind::Eq)
        | Some(TokenKind::Arrow)
        | Some(TokenKind::Return)
        | Some(TokenKind::Case)
        | Some(TokenKind::Default)
        | None => true,
        // After assignment operators
        Some(k) if k.is_assignment() => true,
        // After logical/ternary operators
        Some(TokenKind::AmpAmp)
        | Some(TokenKind::PipePipe)
        | Some(TokenKind::QuestionQuestion)
        | Some(TokenKind::Question) => true,
        // Otherwise, likely comparison
        _ => false,
    }
}

/// Check if a tag name is an intrinsic element (lowercase) or component (uppercase).
pub fn is_intrinsic_element(name: &str) -> bool {
    name.chars()
        .next()
        .map(|c| c.is_lowercase())
        .unwrap_or(false)
}

// =============================================================================
// JSX Parsing (extension methods on Parser)
// =============================================================================

impl<'a> Parser<'a> {
    /// Parse a JSX element or fragment starting at `<`.
    /// Called when `<` is detected in expression position and JSX context is active.
    pub(crate) fn parse_jsx_element_or_fragment(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span.start;

        // Consume `<`
        self.advance();

        // Check for fragment: `<>`
        if self.check(&TokenKind::Gt) {
            return self.parse_jsx_fragment(start);
        }

        // Parse element: `<Tag ...>`
        self.parse_jsx_element(start)
    }

    /// Parse a JSX fragment: `<>children</>`
    fn parse_jsx_fragment(&mut self, start: u32) -> Result<Expr, ParseError> {
        // Consume `>`
        self.advance();

        // Parse children
        let children = self.parse_jsx_children()?;

        // Expect `</>`
        self.expect_jsx_close_fragment()?;

        let end = self.current.span.start;
        Ok(Expr::new(
            ExprKind::JsxFragment(Box::new(JsxFragment {
                children,
                span: Span::new(start, end),
            })),
            Span::new(start, end),
        ))
    }

    /// Parse a JSX element: `<Tag attrs>children</Tag>` or `<Tag attrs />`
    fn parse_jsx_element(&mut self, start: u32) -> Result<Expr, ParseError> {
        // Parse tag name
        let name = self.parse_jsx_element_name()?;

        // Parse attributes
        let attributes = self.parse_jsx_attributes()?;

        // Check for self-closing `/>` or closing `>`
        if self.check(&TokenKind::Slash) {
            // Self-closing: `<Tag ... />`
            self.advance(); // consume `/`
            if !self.check(&TokenKind::Gt) {
                return Err(ParseError::new("Expected '>' after '/'", self.current.span));
            }
            self.advance(); // consume `>`

            let end = self.current.span.start;
            return Ok(Expr::new(
                ExprKind::JsxElement(Box::new(JsxElement {
                    opening: JsxOpeningElement {
                        name,
                        attributes,
                        self_closing: true,
                        span: Span::new(start, end),
                    },
                    children: Vec::new(),
                    closing: None,
                    span: Span::new(start, end),
                })),
                Span::new(start, end),
            ));
        }

        // Expect `>`
        if !self.check(&TokenKind::Gt) {
            return Err(ParseError::new(
                format!("Expected '>' or '/>' in JSX element, got {:?}", self.peek()),
                self.current.span,
            ));
        }
        self.advance(); // consume `>`
        let opening_end = self.current.span.start;

        // Parse children
        let children = self.parse_jsx_children()?;

        // Parse closing tag: `</Tag>`
        let close_start = self.current.span.start;
        self.expect_jsx_close_tag(&name)?;
        let end = self.current.span.start;

        Ok(Expr::new(
            ExprKind::JsxElement(Box::new(JsxElement {
                opening: JsxOpeningElement {
                    name: name.clone(),
                    attributes,
                    self_closing: false,
                    span: Span::new(start, opening_end),
                },
                children,
                closing: Some(JsxClosingElement {
                    name,
                    span: Span::new(close_start, end),
                }),
                span: Span::new(start, end),
            })),
            Span::new(start, end),
        ))
    }

    /// Parse a JSX element name: `div`, `Foo`, `Foo.Bar`, `foo:bar`
    fn parse_jsx_element_name(&mut self) -> Result<JsxElementName, ParseError> {
        let first = self.expect_jsx_identifier()?;

        // Check for namespaced name: `foo:bar`
        if self.check(&TokenKind::Colon) {
            self.advance(); // consume `:`
            let name = self.expect_jsx_identifier()?;
            return Ok(JsxElementName::NamespacedName {
                namespace: first,
                name,
            });
        }

        // Check for member expression: `Foo.Bar.Baz`
        if self.check(&TokenKind::Dot) {
            let mut parts = vec![first];
            while self.eat(&TokenKind::Dot) {
                parts.push(self.expect_jsx_identifier()?);
            }
            return Ok(JsxElementName::MemberExpr(parts));
        }

        Ok(JsxElementName::Ident(first))
    }

    /// Parse JSX attributes: `key="value" onClick={handler} {...spread}`
    fn parse_jsx_attributes(&mut self) -> Result<Vec<JsxAttribute>, ParseError> {
        let mut attributes = Vec::new();

        loop {
            // Stop at `>`, `/>`, or EOF
            if self.check(&TokenKind::Gt) || self.check(&TokenKind::Slash) || self.is_eof() {
                break;
            }

            // Spread attribute: `{...expr}`
            if self.check(&TokenKind::LBrace) {
                let attr_start = self.current.span.start;
                self.advance(); // consume `{`
                if !self.eat(&TokenKind::Spread) {
                    return Err(ParseError::new(
                        "Expected '...' in JSX spread attribute",
                        self.current.span,
                    ));
                }
                let argument = self.parse_assign_expr()?;
                self.expect(&TokenKind::RBrace)?;
                let attr_end = self.current.span.start;
                attributes.push(JsxAttribute::SpreadAttribute {
                    argument,
                    span: Span::new(attr_start, attr_end),
                });
                continue;
            }

            // Named attribute: `name` or `name="value"` or `name={expr}`
            let attr_start = self.current.span.start;
            let attr_name = self.parse_jsx_attr_name()?;

            // Check for `=` (attribute value)
            let value = if self.eat(&TokenKind::Eq) {
                Some(self.parse_jsx_attr_value()?)
            } else {
                None // Boolean attribute: `<input disabled />`
            };

            let attr_end = self.current.span.start;
            attributes.push(JsxAttribute::Attribute {
                name: attr_name,
                value,
                span: Span::new(attr_start, attr_end),
            });
        }

        Ok(attributes)
    }

    /// Parse a JSX attribute name: `className` or `xmlns:xlink`
    fn parse_jsx_attr_name(&mut self) -> Result<JsxAttrName, ParseError> {
        let name = self.expect_jsx_identifier()?;

        // Check for namespaced name
        if self.check(&TokenKind::Colon) {
            self.advance();
            let local = self.expect_jsx_identifier()?;
            return Ok(JsxAttrName::NamespacedName {
                namespace: name,
                name: local,
            });
        }

        Ok(JsxAttrName::Ident(name))
    }

    /// Parse a JSX attribute value: `"string"`, `{expr}`, `<Element />`
    fn parse_jsx_attr_value(&mut self) -> Result<JsxAttrValue, ParseError> {
        // String literal: `"value"` or `'value'`
        if let TokenKind::String(s) = self.peek().clone() {
            self.advance();
            return Ok(JsxAttrValue::String(s));
        }

        // Expression container: `{expr}`
        if self.check(&TokenKind::LBrace) {
            self.advance(); // consume `{`
            let expr = self.parse_assign_expr()?;
            self.expect(&TokenKind::RBrace)?;
            return Ok(JsxAttrValue::Expr(expr));
        }

        // Nested JSX element as value
        if self.check(&TokenKind::Lt) {
            let expr = self.parse_jsx_element_or_fragment()?;
            match expr.kind {
                ExprKind::JsxElement(el) => return Ok(JsxAttrValue::Element(el)),
                ExprKind::JsxFragment(frag) => return Ok(JsxAttrValue::Fragment(frag)),
                _ => {}
            }
        }

        Err(ParseError::new(
            "Expected JSX attribute value (string, {expression}, or <element>)",
            self.current.span,
        ))
    }

    /// Parse JSX children until `</` is encountered.
    fn parse_jsx_children(&mut self) -> Result<Vec<JsxChild>, ParseError> {
        let mut children = Vec::new();

        loop {
            if self.is_eof() {
                return Err(ParseError::new(
                    "Unterminated JSX element",
                    self.current.span,
                ));
            }

            // Check for closing tag or end of fragment
            if self.check(&TokenKind::Lt) {
                // Look ahead: if next is `/`, this is a closing tag
                let next_src = &self.source[self.current.span.end as usize..];
                let next_char: Option<char> = next_src.trim_start().chars().next();
                if next_char == Some('/') {
                    break; // Closing tag - stop collecting children
                }

                // Otherwise, it's a nested element
                let child_expr = self.parse_jsx_element_or_fragment()?;
                match child_expr.kind {
                    ExprKind::JsxElement(el) => children.push(JsxChild::Element(el)),
                    ExprKind::JsxFragment(frag) => children.push(JsxChild::Fragment(frag)),
                    _ => {}
                }
                continue;
            }

            // Expression container: `{expr}` or `{...expr}`
            if self.check(&TokenKind::LBrace) {
                self.advance(); // consume `{`
                if self.eat(&TokenKind::Spread) {
                    let expr = self.parse_assign_expr()?;
                    self.expect(&TokenKind::RBrace)?;
                    children.push(JsxChild::Spread(expr));
                } else {
                    let expr = self.parse_assign_expr()?;
                    self.expect(&TokenKind::RBrace)?;
                    children.push(JsxChild::Expr(expr));
                }
                continue;
            }

            // Text content - collect everything until `<`, `{`, or EOF
            let _text_start = self.current.span.start;
            let mut text = String::new();
            let mut found_text = false;

            // Consume tokens as text until we hit JSX-significant tokens
            while !self.is_eof() && !self.check(&TokenKind::Lt) && !self.check(&TokenKind::LBrace) {
                // Get the source text for this token
                let token_text =
                    &self.source[self.current.span.start as usize..self.current.span.end as usize];
                text.push_str(token_text);
                found_text = true;
                self.advance();
            }

            if found_text {
                // Trim insignificant whitespace from JSX text
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    children.push(JsxChild::Text(trimmed.to_string()));
                }
            } else {
                // Safety: avoid infinite loop
                break;
            }
        }

        Ok(children)
    }

    /// Expect a closing tag `</Tag>` matching the opening tag name.
    fn expect_jsx_close_tag(&mut self, expected_name: &JsxElementName) -> Result<(), ParseError> {
        // Consume `<`
        if !self.check(&TokenKind::Lt) {
            return Err(ParseError::new(
                "Expected closing tag for JSX element".to_string(),
                self.current.span,
            ));
        }
        // Tell the lexer that `/` in `</Tag>` is not a regex
        self.lexer.set_no_regex();
        self.advance(); // consume `<`

        // Consume `/`
        if !self.check(&TokenKind::Slash) {
            return Err(ParseError::new(
                "Expected '/' in closing tag",
                self.current.span,
            ));
        }
        self.advance(); // consume `/`

        // Parse and verify the tag name
        let close_name = self.parse_jsx_element_name()?;
        if close_name != *expected_name {
            return Err(ParseError::new(
                "Mismatched closing tag".to_string(),
                self.current.span,
            ));
        }

        // Consume `>`
        if !self.check(&TokenKind::Gt) {
            return Err(ParseError::new(
                "Expected '>' in closing tag",
                self.current.span,
            ));
        }
        self.advance(); // consume `>`

        Ok(())
    }

    /// Expect a closing fragment `</>`.
    fn expect_jsx_close_fragment(&mut self) -> Result<(), ParseError> {
        // Consume `<`
        if !self.check(&TokenKind::Lt) {
            return Err(ParseError::new(
                "Expected '<' for closing fragment",
                self.current.span,
            ));
        }
        // Tell the lexer that `/` in `</>` is not a regex
        self.lexer.set_no_regex();
        self.advance();

        // Consume `/`
        if !self.check(&TokenKind::Slash) {
            return Err(ParseError::new(
                "Expected '/' in closing fragment",
                self.current.span,
            ));
        }
        self.advance();

        // Consume `>`
        if !self.check(&TokenKind::Gt) {
            return Err(ParseError::new(
                "Expected '>' in closing fragment",
                self.current.span,
            ));
        }
        self.advance();

        Ok(())
    }

    /// Expect a JSX identifier (accepts keywords as identifiers in JSX context).
    fn expect_jsx_identifier(&mut self) -> Result<String, ParseError> {
        match self.peek().clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            // In JSX, keywords can be used as identifiers (e.g., `<div class="foo">`)
            ref k if k.is_keyword() => {
                let name = format!("{:?}", k).to_lowercase();
                self.advance();
                Ok(name)
            }
            _ => Err(ParseError::new(
                format!("Expected JSX identifier, got {:?}", self.peek()),
                self.current.span,
            )),
        }
    }
}

// =============================================================================
// JSX Code Generation
// =============================================================================

/// JSX runtime configuration.
#[derive(Debug, Clone, Default)]
pub enum JsxRuntime {
    /// Classic runtime: React.createElement
    Classic,
    /// Automatic runtime (React 17+): jsx/jsxs from react/jsx-runtime
    #[default]
    Automatic,
}

/// JSX transform options.
#[derive(Debug, Clone, Default)]
pub struct JsxOptions {
    /// JSX runtime to use.
    pub runtime: JsxRuntime,
    /// Pragma for classic runtime (default: "React.createElement").
    pub pragma: Option<String>,
    /// Pragma fragment for classic runtime (default: "React.Fragment").
    pub pragma_frag: Option<String>,
    /// Import source for automatic runtime (default: "react").
    pub import_source: Option<String>,
    /// Whether this is a development build (adds debugging info).
    pub development: bool,
}

/// Generate code for a JSX element, transforming to function calls.
pub fn emit_jsx_element(element: &JsxElement, out: &mut String) {
    let tag = element_name_to_string(&element.opening.name);
    let has_multiple_children = element.children.len() > 1;

    // Use _jsxs for multiple children, _jsx for single/no children
    if has_multiple_children {
        out.push_str("_jsxs(");
    } else {
        out.push_str("_jsx(");
    }

    // Tag name: "div" for intrinsic, Component for components
    if is_intrinsic_element(&tag) {
        out.push('"');
        out.push_str(&tag);
        out.push('"');
    } else {
        out.push_str(&tag);
    }

    out.push_str(", ");

    // Props object
    emit_jsx_props(
        &element.opening.attributes,
        &element.children,
        has_multiple_children,
        out,
    );

    out.push(')');
}

/// Generate code for a JSX fragment.
pub fn emit_jsx_fragment(fragment: &JsxFragment, out: &mut String) {
    let has_multiple_children = fragment.children.len() > 1;

    if has_multiple_children {
        out.push_str("_jsxs(_Fragment, ");
    } else {
        out.push_str("_jsx(_Fragment, ");
    }

    // Props with children
    emit_jsx_props(&[], &fragment.children, has_multiple_children, out);

    out.push(')');
}

/// Emit JSX props object including children.
fn emit_jsx_props(
    attributes: &[JsxAttribute],
    children: &[JsxChild],
    multiple_children: bool,
    out: &mut String,
) {
    out.push('{');
    let mut first = true;

    // Emit attributes
    for attr in attributes {
        match attr {
            JsxAttribute::Attribute { name, value, .. } => {
                if !first {
                    out.push_str(", ");
                }
                first = false;
                emit_jsx_attr_name(name, out);
                out.push_str(": ");
                match value {
                    Some(JsxAttrValue::String(s)) => {
                        out.push('"');
                        out.push_str(s);
                        out.push('"');
                    }
                    Some(JsxAttrValue::Expr(_expr)) => {
                        // For expr values, we'd need to emit the expression
                        // For now, use a placeholder that the codegen fills in
                        out.push_str("__JSX_EXPR__");
                    }
                    Some(JsxAttrValue::Element(el)) => {
                        emit_jsx_element(el, out);
                    }
                    Some(JsxAttrValue::Fragment(frag)) => {
                        emit_jsx_fragment(frag, out);
                    }
                    None => {
                        out.push_str("true"); // Boolean attribute
                    }
                }
            }
            JsxAttribute::SpreadAttribute { .. } => {
                // Spread attributes need special handling
                if !first {
                    out.push_str(", ");
                }
                first = false;
                out.push_str("...__JSX_SPREAD__");
            }
        }
    }

    // Emit children
    if !children.is_empty() {
        if !first {
            out.push_str(", ");
        }
        out.push_str("children: ");
        if multiple_children {
            out.push('[');
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                emit_jsx_child(child, out);
            }
            out.push(']');
        } else if let Some(child) = children.first() {
            emit_jsx_child(child, out);
        }
    }

    out.push('}');
}

/// Emit a JSX child.
fn emit_jsx_child(child: &JsxChild, out: &mut String) {
    match child {
        JsxChild::Text(text) => {
            out.push('"');
            // Escape the text
            for c in text.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    _ => out.push(c),
                }
            }
            out.push('"');
        }
        JsxChild::Element(el) => emit_jsx_element(el, out),
        JsxChild::Fragment(frag) => emit_jsx_fragment(frag, out),
        JsxChild::Expr(_) => out.push_str("__JSX_EXPR__"),
        JsxChild::Spread(_) => out.push_str("...__JSX_SPREAD__"),
    }
}

/// Emit a JSX attribute name.
fn emit_jsx_attr_name(name: &JsxAttrName, out: &mut String) {
    match name {
        JsxAttrName::Ident(s) => out.push_str(s),
        JsxAttrName::NamespacedName { namespace, name } => {
            out.push('"');
            out.push_str(namespace);
            out.push(':');
            out.push_str(name);
            out.push('"');
        }
    }
}

/// Convert a JSX element name to its string representation.
fn element_name_to_string(name: &JsxElementName) -> String {
    match name {
        JsxElementName::Ident(s) => s.clone(),
        JsxElementName::MemberExpr(parts) => parts.join("."),
        JsxElementName::NamespacedName { namespace, name } => {
            format!("{}:{}", namespace, name)
        }
    }
}
