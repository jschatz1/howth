//! JavaScript code generator.
//!
//! Converts an AST back to JavaScript source code.
//! Supports minification and source map generation.

use crate::ast::*;
use crate::span::Span;
use std::collections::HashMap;

/// Code generation options.
#[derive(Debug, Clone, Default)]
pub struct CodegenOptions {
    /// Minify output (remove whitespace, shorten names).
    pub minify: bool,
    /// Generate source maps.
    pub source_map: bool,
    /// Indent string (default: "  ").
    pub indent: Option<String>,
    /// Target ECMAScript version.
    pub target: Target,
}

/// ECMAScript target version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Target {
    /// ES5 (IE11 compatible)
    ES5,
    /// ES2015 (ES6)
    ES2015,
    /// ES2016
    ES2016,
    /// ES2017
    ES2017,
    /// ES2018
    ES2018,
    /// ES2019
    ES2019,
    /// ES2020
    ES2020,
    /// ES2021
    ES2021,
    /// ES2022
    ES2022,
    /// ESNext (latest)
    #[default]
    ESNext,
}

/// The code generator.
pub struct Codegen<'a> {
    /// The AST to generate code from.
    ast: &'a Ast,
    /// Code generation options.
    options: CodegenOptions,
    /// Output buffer.
    output: String,
    /// Current indentation level.
    indent_level: usize,
    /// Indent string.
    indent_str: String,
    /// Whether we need a space before the next token.
    needs_space: bool,
    /// Whether we need a semicolon.
    needs_semicolon: bool,
    /// Source map mappings (if enabled).
    mappings: Vec<SourceMapping>,
    /// Identifier renames (for scope hoisting).
    renames: HashMap<String, String>,
}

/// A source map mapping.
#[derive(Debug, Clone)]
pub struct SourceMapping {
    /// Generated line (0-indexed).
    pub gen_line: u32,
    /// Generated column (0-indexed).
    pub gen_col: u32,
    /// Original byte offset.
    pub orig_offset: u32,
}

impl<'a> Codegen<'a> {
    /// Create a new code generator.
    pub fn new(ast: &'a Ast, options: CodegenOptions) -> Self {
        let indent_str = options.indent.clone().unwrap_or_else(|| "  ".to_string());
        Self {
            ast,
            options,
            output: String::new(),
            indent_level: 0,
            indent_str,
            needs_space: false,
            needs_semicolon: false,
            mappings: Vec::new(),
            renames: HashMap::new(),
        }
    }

    /// Create a code generator with identifier renames (for scope hoisting).
    pub fn with_renames(ast: &'a Ast, options: CodegenOptions, renames: HashMap<String, String>) -> Self {
        let indent_str = options.indent.clone().unwrap_or_else(|| "  ".to_string());
        Self {
            ast,
            options,
            output: String::new(),
            indent_level: 0,
            indent_str,
            needs_space: false,
            needs_semicolon: false,
            mappings: Vec::new(),
            renames,
        }
    }

    /// Rename an identifier if it's in the renames map.
    fn rename(&self, name: &str) -> String {
        self.renames.get(name).cloned().unwrap_or_else(|| name.to_string())
    }

    /// Generate JavaScript source code.
    pub fn generate(mut self) -> String {
        for stmt in &self.ast.stmts {
            self.emit_stmt(stmt);
            if !self.options.minify {
                self.emit_newline();
            }
        }
        self.output
    }

    /// Generate JavaScript source code with source map.
    pub fn generate_with_source_map(mut self) -> (String, Vec<SourceMapping>) {
        for stmt in &self.ast.stmts {
            self.emit_stmt(stmt);
            if !self.options.minify {
                self.emit_newline();
            }
        }
        (self.output, self.mappings)
    }

    // =========================================================================
    // Output Helpers
    // =========================================================================

    fn emit(&mut self, s: &str) {
        if self.needs_semicolon {
            self.output.push(';');
            self.needs_semicolon = false;
        }
        if self.needs_space && !s.is_empty() {
            let first = s.chars().next().unwrap();
            if first.is_alphanumeric() || first == '_' || first == '$' {
                self.output.push(' ');
            }
            self.needs_space = false;
        }
        self.output.push_str(s);
    }

    fn emit_space(&mut self) {
        if !self.options.minify {
            self.output.push(' ');
        }
    }

    fn emit_space_or_newline(&mut self) {
        if self.options.minify {
            self.output.push(' ');
        } else {
            self.emit_newline();
        }
    }

    fn emit_newline(&mut self) {
        if !self.options.minify {
            self.output.push('\n');
            for _ in 0..self.indent_level {
                self.output.push_str(&self.indent_str);
            }
        }
    }

    fn emit_semicolon(&mut self) {
        if self.options.minify {
            self.needs_semicolon = true;
        } else {
            self.output.push(';');
        }
    }

    fn emit_with_mapping(&mut self, s: &str, span: Span) {
        if self.options.source_map {
            let lines: Vec<&str> = self.output.split('\n').collect();
            let gen_line = (lines.len() - 1) as u32;
            let gen_col = lines.last().map(|l| l.len() as u32).unwrap_or(0);
            self.mappings.push(SourceMapping {
                gen_line,
                gen_col,
                orig_offset: span.start,
            });
        }
        self.emit(s);
    }

    fn indent(&mut self) {
        self.indent_level += 1;
    }

    fn dedent(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    // =========================================================================
    // Statement Emission
    // =========================================================================

    fn emit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Var { kind, decls } => {
                self.emit_var_decl(*kind, decls);
            }
            StmtKind::Function(func) => {
                self.emit_function(func, true);
            }
            StmtKind::Class(class) => {
                self.emit_class(class, true);
            }
            StmtKind::Block(stmts) => {
                self.emit_block(stmts);
            }
            StmtKind::If { test, consequent, alternate } => {
                self.emit("if");
                self.emit_space();
                self.emit("(");
                self.emit_expr(test);
                self.emit(")");
                self.emit_space();
                self.emit_stmt(consequent);
                if let Some(alt) = alternate {
                    self.emit_space();
                    self.emit("else");
                    self.emit_space();
                    self.emit_stmt(alt);
                }
            }
            StmtKind::Switch { discriminant, cases } => {
                self.emit("switch");
                self.emit_space();
                self.emit("(");
                self.emit_expr(discriminant);
                self.emit(")");
                self.emit_space();
                self.emit("{");
                self.indent();
                for case in cases {
                    self.emit_newline();
                    if let Some(test) = &case.test {
                        self.emit("case");
                        self.emit(" ");
                        self.emit_expr(test);
                        self.emit(":");
                    } else {
                        self.emit("default:");
                    }
                    self.indent();
                    for stmt in &case.consequent {
                        self.emit_newline();
                        self.emit_stmt(stmt);
                    }
                    self.dedent();
                }
                self.dedent();
                self.emit_newline();
                self.emit("}");
            }
            StmtKind::For { init, test, update, body } => {
                self.emit("for");
                self.emit_space();
                self.emit("(");
                if let Some(init) = init {
                    self.emit_for_init(init);
                }
                self.emit(";");
                if let Some(test) = test {
                    self.emit_space();
                    self.emit_expr(test);
                }
                self.emit(";");
                if let Some(update) = update {
                    self.emit_space();
                    self.emit_expr(update);
                }
                self.emit(")");
                self.emit_space();
                self.emit_stmt(body);
            }
            StmtKind::ForIn { left, right, body } => {
                self.emit("for");
                self.emit_space();
                self.emit("(");
                self.emit_for_init(left);
                self.emit(" in ");
                self.emit_expr(right);
                self.emit(")");
                self.emit_space();
                self.emit_stmt(body);
            }
            StmtKind::ForOf { left, right, body, is_await } => {
                self.emit("for");
                if *is_await {
                    self.emit(" await");
                }
                self.emit_space();
                self.emit("(");
                self.emit_for_init(left);
                self.emit(" of ");
                self.emit_expr(right);
                self.emit(")");
                self.emit_space();
                self.emit_stmt(body);
            }
            StmtKind::While { test, body } => {
                self.emit("while");
                self.emit_space();
                self.emit("(");
                self.emit_expr(test);
                self.emit(")");
                self.emit_space();
                self.emit_stmt(body);
            }
            StmtKind::DoWhile { body, test } => {
                self.emit("do");
                self.emit_space();
                self.emit_stmt(body);
                self.emit_space();
                self.emit("while");
                self.emit_space();
                self.emit("(");
                self.emit_expr(test);
                self.emit(")");
                self.emit_semicolon();
            }
            StmtKind::Break { label } => {
                self.emit("break");
                if let Some(label) = label {
                    self.emit(" ");
                    self.emit(label);
                }
                self.emit_semicolon();
            }
            StmtKind::Continue { label } => {
                self.emit("continue");
                if let Some(label) = label {
                    self.emit(" ");
                    self.emit(label);
                }
                self.emit_semicolon();
            }
            StmtKind::Return { arg } => {
                self.emit("return");
                if let Some(arg) = arg {
                    self.emit(" ");
                    self.emit_expr(arg);
                }
                self.emit_semicolon();
            }
            StmtKind::Throw { arg } => {
                self.emit("throw");
                self.emit(" ");
                self.emit_expr(arg);
                self.emit_semicolon();
            }
            StmtKind::Try { block, handler, finalizer } => {
                self.emit("try");
                self.emit_space();
                self.emit_block(block);
                if let Some(catch) = handler {
                    self.emit_space();
                    self.emit("catch");
                    if let Some(param) = &catch.param {
                        self.emit_space();
                        self.emit("(");
                        self.emit_binding(param);
                        self.emit(")");
                    }
                    self.emit_space();
                    self.emit_block(&catch.body);
                }
                if let Some(finally) = finalizer {
                    self.emit_space();
                    self.emit("finally");
                    self.emit_space();
                    self.emit_block(finally);
                }
            }
            StmtKind::Labeled { label, body } => {
                self.emit(label);
                self.emit(":");
                self.emit_space();
                self.emit_stmt(body);
            }
            StmtKind::Expr(expr) => {
                self.emit_expr(expr);
                self.emit_semicolon();
            }
            StmtKind::Empty => {
                self.emit(";");
            }
            StmtKind::Debugger => {
                self.emit("debugger");
                self.emit_semicolon();
            }
            StmtKind::With { object, body } => {
                self.emit("with");
                self.emit_space();
                self.emit("(");
                self.emit_expr(object);
                self.emit(")");
                self.emit_space();
                self.emit_stmt(body);
            }
            StmtKind::Import(decl) => {
                self.emit_import(decl);
            }
            StmtKind::Export(decl) => {
                self.emit_export(decl);
            }
            #[cfg(feature = "typescript")]
            _ => {
                // TypeScript-specific statements
                // TODO: Implement TypeScript code generation
            }
        }
    }

    fn emit_block(&mut self, stmts: &[Stmt]) {
        self.emit("{");
        if !stmts.is_empty() {
            self.indent();
            for stmt in stmts {
                self.emit_newline();
                self.emit_stmt(stmt);
            }
            self.dedent();
            self.emit_newline();
        }
        self.emit("}");
    }

    fn emit_var_decl(&mut self, kind: VarKind, decls: &[VarDeclarator]) {
        match kind {
            VarKind::Var => self.emit("var"),
            VarKind::Let => self.emit("let"),
            VarKind::Const => self.emit("const"),
        }
        self.emit(" ");

        for (i, decl) in decls.iter().enumerate() {
            if i > 0 {
                self.emit(",");
                self.emit_space();
            }
            self.emit_binding(&decl.binding);
            if let Some(init) = &decl.init {
                self.emit_space();
                self.emit("=");
                self.emit_space();
                self.emit_expr(init);
            }
        }
        self.emit_semicolon();
    }

    fn emit_for_init(&mut self, init: &ForInit) {
        match init {
            ForInit::Var { kind, decls } => {
                match kind {
                    VarKind::Var => self.emit("var"),
                    VarKind::Let => self.emit("let"),
                    VarKind::Const => self.emit("const"),
                }
                self.emit(" ");
                for (i, decl) in decls.iter().enumerate() {
                    if i > 0 {
                        self.emit(",");
                        self.emit_space();
                    }
                    self.emit_binding(&decl.binding);
                    if let Some(init) = &decl.init {
                        self.emit_space();
                        self.emit("=");
                        self.emit_space();
                        self.emit_expr(init);
                    }
                }
            }
            ForInit::Expr(expr) => {
                self.emit_expr(expr);
            }
        }
    }

    fn emit_binding(&mut self, binding: &Binding) {
        match &binding.kind {
            BindingKind::Ident { name, .. } => {
                let renamed = self.rename(name);
                self.emit(&renamed);
            }
            BindingKind::Array { elements, .. } => {
                self.emit("[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.emit(",");
                        self.emit_space();
                    }
                    if let Some(elem) = elem {
                        if elem.rest {
                            self.emit("...");
                        }
                        self.emit_binding(&elem.binding);
                        if let Some(default) = &elem.default {
                            self.emit_space();
                            self.emit("=");
                            self.emit_space();
                            self.emit_expr(default);
                        }
                    }
                }
                self.emit("]");
            }
            BindingKind::Object { properties, .. } => {
                self.emit("{");
                for (i, prop) in properties.iter().enumerate() {
                    if i > 0 {
                        self.emit(",");
                        self.emit_space();
                    }
                    if prop.rest {
                        self.emit("...");
                        self.emit_binding(&prop.value);
                    } else if prop.shorthand {
                        self.emit_binding(&prop.value);
                    } else {
                        self.emit_property_key(&prop.key);
                        self.emit(":");
                        self.emit_space();
                        self.emit_binding(&prop.value);
                    }
                    if let Some(default) = &prop.default {
                        self.emit_space();
                        self.emit("=");
                        self.emit_space();
                        self.emit_expr(default);
                    }
                }
                self.emit("}");
            }
        }
    }

    fn emit_function(&mut self, func: &Function, is_declaration: bool) {
        if func.is_async {
            self.emit("async");
            self.emit(" ");
        }
        self.emit("function");
        if func.is_generator {
            self.emit("*");
        }
        if let Some(name) = &func.name {
            self.emit(" ");
            let renamed = self.rename(name);
            self.emit(&renamed);
        } else if is_declaration {
            self.emit(" ");
        }
        self.emit_params(&func.params);
        self.emit_space();
        self.emit_block(&func.body);
    }

    fn emit_arrow(&mut self, arrow: &ArrowFunction) {
        if arrow.is_async {
            self.emit("async");
            self.emit(" ");
        }

        // Single identifier parameter can omit parens
        if arrow.params.len() == 1 && !arrow.params[0].rest && arrow.params[0].default.is_none() {
            if let BindingKind::Ident { name, .. } = &arrow.params[0].binding.kind {
                let renamed = self.rename(name);
                self.emit(&renamed);
            } else {
                self.emit_params(&arrow.params);
            }
        } else {
            self.emit_params(&arrow.params);
        }

        self.emit_space();
        self.emit("=>");
        self.emit_space();

        match &arrow.body {
            ArrowBody::Expr(expr) => {
                // Object literal needs parens
                if matches!(expr.kind, ExprKind::Object(_)) {
                    self.emit("(");
                    self.emit_expr(expr);
                    self.emit(")");
                } else {
                    self.emit_expr(expr);
                }
            }
            ArrowBody::Block(stmts) => {
                self.emit_block(stmts);
            }
        }
    }

    fn emit_params(&mut self, params: &[Param]) {
        self.emit("(");
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                self.emit(",");
                self.emit_space();
            }
            if param.rest {
                self.emit("...");
            }
            self.emit_binding(&param.binding);
            if let Some(default) = &param.default {
                self.emit_space();
                self.emit("=");
                self.emit_space();
                self.emit_expr(default);
            }
        }
        self.emit(")");
    }

    fn emit_class(&mut self, class: &Class, is_declaration: bool) {
        self.emit("class");
        if let Some(name) = &class.name {
            self.emit(" ");
            let renamed = self.rename(name);
            self.emit(&renamed);
        } else if is_declaration {
            self.emit(" ");
        }
        if let Some(super_class) = &class.super_class {
            self.emit(" extends ");
            self.emit_expr(super_class);
        }
        self.emit_space();
        self.emit("{");
        self.indent();
        for member in &class.body {
            self.emit_newline();
            self.emit_class_member(member);
        }
        self.dedent();
        if !class.body.is_empty() {
            self.emit_newline();
        }
        self.emit("}");
    }

    fn emit_class_member(&mut self, member: &ClassMember) {
        match &member.kind {
            ClassMemberKind::Method { key, value, kind, computed, is_static, .. } => {
                if *is_static {
                    self.emit("static ");
                }
                match kind {
                    MethodKind::Get => self.emit("get "),
                    MethodKind::Set => self.emit("set "),
                    _ => {}
                }
                if value.is_async {
                    self.emit("async ");
                }
                if value.is_generator {
                    self.emit("*");
                }
                if *computed {
                    self.emit("[");
                }
                self.emit_property_key(key);
                if *computed {
                    self.emit("]");
                }
                self.emit_params(&value.params);
                self.emit_space();
                self.emit_block(&value.body);
            }
            ClassMemberKind::Property { key, value, computed, is_static, .. } => {
                if *is_static {
                    self.emit("static ");
                }
                if *computed {
                    self.emit("[");
                }
                self.emit_property_key(key);
                if *computed {
                    self.emit("]");
                }
                if let Some(value) = value {
                    self.emit_space();
                    self.emit("=");
                    self.emit_space();
                    self.emit_expr(value);
                }
                self.emit_semicolon();
            }
            ClassMemberKind::StaticBlock(stmts) => {
                self.emit("static");
                self.emit_space();
                self.emit_block(stmts);
            }
            ClassMemberKind::Empty => {
                self.emit(";");
            }
        }
    }

    fn emit_import(&mut self, decl: &ImportDecl) {
        self.emit("import");
        self.emit(" ");

        let mut has_default = false;
        let mut has_namespace = false;
        let mut named = Vec::new();

        for spec in &decl.specifiers {
            match spec {
                ImportSpecifier::Default { local, .. } => {
                    self.emit(local);
                    has_default = true;
                }
                ImportSpecifier::Namespace { local, .. } => {
                    if has_default {
                        self.emit(",");
                        self.emit_space();
                    }
                    self.emit("*");
                    self.emit_space();
                    self.emit("as");
                    self.emit(" ");
                    self.emit(local);
                    has_namespace = true;
                }
                ImportSpecifier::Named { imported, local, .. } => {
                    named.push((imported, local));
                }
            }
        }

        if !named.is_empty() {
            if has_default || has_namespace {
                self.emit(",");
                self.emit_space();
            }
            self.emit("{");
            for (i, (imported, local)) in named.iter().enumerate() {
                if i > 0 {
                    self.emit(",");
                    self.emit_space();
                }
                if imported == local {
                    self.emit(local);
                } else {
                    self.emit(imported);
                    self.emit(" as ");
                    self.emit(local);
                }
            }
            self.emit("}");
        }

        if has_default || has_namespace || !named.is_empty() {
            self.emit(" from ");
        }
        self.emit("\"");
        self.emit(&decl.source);
        self.emit("\"");
        self.emit_semicolon();
    }

    fn emit_export(&mut self, decl: &ExportDecl) {
        match decl {
            ExportDecl::Named { specifiers, source, .. } => {
                self.emit("export");
                self.emit(" ");
                self.emit("{");
                for (i, spec) in specifiers.iter().enumerate() {
                    if i > 0 {
                        self.emit(",");
                        self.emit_space();
                    }
                    if spec.local == spec.exported {
                        self.emit(&spec.local);
                    } else {
                        self.emit(&spec.local);
                        self.emit(" as ");
                        self.emit(&spec.exported);
                    }
                }
                self.emit("}");
                if let Some(source) = source {
                    self.emit(" from \"");
                    self.emit(source);
                    self.emit("\"");
                }
                self.emit_semicolon();
            }
            ExportDecl::Default { expr, .. } => {
                self.emit("export default ");
                self.emit_expr(expr);
                self.emit_semicolon();
            }
            ExportDecl::Decl { decl, .. } => {
                self.emit("export ");
                self.emit_stmt(decl);
            }
            ExportDecl::All { exported, source, .. } => {
                self.emit("export *");
                if let Some(exported) = exported {
                    self.emit(" as ");
                    self.emit(exported);
                }
                self.emit(" from \"");
                self.emit(source);
                self.emit("\"");
                self.emit_semicolon();
            }
        }
    }

    // =========================================================================
    // Expression Emission
    // =========================================================================

    fn emit_expr(&mut self, expr: &Expr) {
        self.emit_expr_with_prec(expr, 0);
    }

    fn emit_expr_with_prec(&mut self, expr: &Expr, min_prec: u8) {
        match &expr.kind {
            ExprKind::Null => self.emit("null"),
            ExprKind::Bool(b) => self.emit(if *b { "true" } else { "false" }),
            ExprKind::Number(n) => {
                // Handle special float values
                if n.is_nan() {
                    self.emit("NaN");
                } else if n.is_infinite() {
                    if n.is_sign_positive() {
                        self.emit("Infinity");
                    } else {
                        self.emit("-Infinity");
                    }
                } else {
                    self.emit(&format_number(*n));
                }
            }
            ExprKind::BigInt(s) => {
                self.emit(s);
                self.emit("n");
            }
            ExprKind::String(s) => {
                self.emit("\"");
                self.emit(&escape_string(s));
                self.emit("\"");
            }
            ExprKind::Regex { pattern, flags } => {
                self.emit("/");
                self.emit(pattern);
                self.emit("/");
                self.emit(flags);
            }
            ExprKind::TemplateNoSub(s) => {
                self.emit("`");
                self.emit(&escape_template(s));
                self.emit("`");
            }
            ExprKind::Template { quasis, exprs } => {
                self.emit("`");
                for (i, quasi) in quasis.iter().enumerate() {
                    self.emit(&escape_template(quasi));
                    if i < exprs.len() {
                        self.emit("${");
                        self.emit_expr(&exprs[i]);
                        self.emit("}");
                    }
                }
                self.emit("`");
            }
            ExprKind::Ident(name) => {
                let renamed = self.rename(name);
                self.emit(&renamed);
            }
            ExprKind::This => self.emit("this"),
            ExprKind::Super => self.emit("super"),
            ExprKind::Array(elements) => {
                self.emit("[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.emit(",");
                        self.emit_space();
                    }
                    if let Some(elem) = elem {
                        self.emit_expr(elem);
                    }
                }
                self.emit("]");
            }
            ExprKind::Object(properties) => {
                self.emit("{");
                for (i, prop) in properties.iter().enumerate() {
                    if i > 0 {
                        self.emit(",");
                        self.emit_space();
                    }
                    self.emit_object_property(prop);
                }
                self.emit("}");
            }
            ExprKind::Function(func) => {
                self.emit_function(func, false);
            }
            ExprKind::Arrow(arrow) => {
                // Arrows have low precedence; may need parens
                if min_prec > 0 {
                    self.emit("(");
                }
                self.emit_arrow(arrow);
                if min_prec > 0 {
                    self.emit(")");
                }
            }
            ExprKind::Class(class) => {
                self.emit_class(class, false);
            }
            ExprKind::Unary { op, arg } => {
                let op_str = match op {
                    UnaryOp::Minus => "-",
                    UnaryOp::Plus => "+",
                    UnaryOp::Not => "!",
                    UnaryOp::BitNot => "~",
                    UnaryOp::Typeof => "typeof ",
                    UnaryOp::Void => "void ",
                    UnaryOp::Delete => "delete ",
                };
                self.emit(op_str);
                self.emit_expr_with_prec(arg, 15); // Unary precedence
            }
            ExprKind::Binary { op, left, right } => {
                let (prec, op_str) = binary_op_info(*op);
                let needs_parens = prec < min_prec;
                if needs_parens {
                    self.emit("(");
                }
                self.emit_expr_with_prec(left, prec);
                self.emit_space();
                self.emit(op_str);
                self.emit_space();
                // Right side needs higher precedence for left-associative ops
                let right_prec = if is_right_associative(*op) { prec } else { prec + 1 };
                self.emit_expr_with_prec(right, right_prec);
                if needs_parens {
                    self.emit(")");
                }
            }
            ExprKind::Assign { op, left, right } => {
                if min_prec > 2 {
                    self.emit("(");
                }
                self.emit_expr_with_prec(left, 3);
                self.emit_space();
                self.emit(assign_op_str(*op));
                self.emit_space();
                self.emit_expr_with_prec(right, 2);
                if min_prec > 2 {
                    self.emit(")");
                }
            }
            ExprKind::Update { op, prefix, arg } => {
                let op_str = match op {
                    UpdateOp::Increment => "++",
                    UpdateOp::Decrement => "--",
                };
                if *prefix {
                    self.emit(op_str);
                    self.emit_expr_with_prec(arg, 15);
                } else {
                    self.emit_expr_with_prec(arg, 16);
                    self.emit(op_str);
                }
            }
            ExprKind::Conditional { test, consequent, alternate } => {
                if min_prec > 3 {
                    self.emit("(");
                }
                self.emit_expr_with_prec(test, 4);
                self.emit_space();
                self.emit("?");
                self.emit_space();
                self.emit_expr_with_prec(consequent, 2);
                self.emit_space();
                self.emit(":");
                self.emit_space();
                self.emit_expr_with_prec(alternate, 2);
                if min_prec > 3 {
                    self.emit(")");
                }
            }
            ExprKind::Sequence(exprs) => {
                for (i, expr) in exprs.iter().enumerate() {
                    if i > 0 {
                        self.emit(",");
                        self.emit_space();
                    }
                    self.emit_expr_with_prec(expr, 1);
                }
            }
            ExprKind::Member { object, property, computed } => {
                self.emit_expr_with_prec(object, 18);
                if *computed {
                    self.emit("[");
                    self.emit_expr(property);
                    self.emit("]");
                } else {
                    self.emit(".");
                    self.emit_expr(property);
                }
            }
            ExprKind::OptionalMember { object, property, computed } => {
                self.emit_expr_with_prec(object, 18);
                if *computed {
                    self.emit("?.[");
                    self.emit_expr(property);
                    self.emit("]");
                } else {
                    self.emit("?.");
                    self.emit_expr(property);
                }
            }
            ExprKind::Call { callee, args } => {
                self.emit_expr_with_prec(callee, 18);
                self.emit("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.emit(",");
                        self.emit_space();
                    }
                    self.emit_expr_with_prec(arg, 2);
                }
                self.emit(")");
            }
            ExprKind::OptionalCall { callee, args } => {
                self.emit_expr_with_prec(callee, 18);
                self.emit("?.(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.emit(",");
                        self.emit_space();
                    }
                    self.emit_expr_with_prec(arg, 2);
                }
                self.emit(")");
            }
            ExprKind::New { callee, args } => {
                self.emit("new ");
                self.emit_expr_with_prec(callee, 17);
                if !args.is_empty() {
                    self.emit("(");
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            self.emit(",");
                            self.emit_space();
                        }
                        self.emit_expr_with_prec(arg, 2);
                    }
                    self.emit(")");
                }
            }
            ExprKind::TaggedTemplate { tag, quasi } => {
                self.emit_expr_with_prec(tag, 18);
                self.emit_expr(quasi);
            }
            ExprKind::Spread(arg) => {
                self.emit("...");
                self.emit_expr_with_prec(arg, 2);
            }
            ExprKind::Yield { arg, delegate } => {
                self.emit("yield");
                if *delegate {
                    self.emit("*");
                }
                if let Some(arg) = arg {
                    self.emit(" ");
                    self.emit_expr_with_prec(arg, 2);
                }
            }
            ExprKind::Await(arg) => {
                self.emit("await ");
                self.emit_expr_with_prec(arg, 15);
            }
            ExprKind::Import(arg) => {
                self.emit("import(");
                self.emit_expr(arg);
                self.emit(")");
            }
            ExprKind::MetaProperty { meta, property } => {
                self.emit(meta);
                self.emit(".");
                self.emit(property);
            }
            #[cfg(feature = "jsx")]
            ExprKind::JsxElement(el) => {
                self.emit_jsx_element(el);
            }
            #[cfg(feature = "jsx")]
            ExprKind::JsxFragment(frag) => {
                self.emit_jsx_fragment(frag);
            }
            #[cfg(feature = "typescript")]
            _ => {
                // TypeScript-specific expressions
                // TODO: Implement TypeScript code generation
            }
        }
    }

    // =========================================================================
    // JSX Code Generation
    // =========================================================================

    #[cfg(feature = "jsx")]
    fn emit_jsx_element(&mut self, element: &JsxElement) {
        let tag = self.jsx_element_name_string(&element.opening.name);
        let has_multiple_children = element.children.len() > 1;

        // _jsx or _jsxs
        if has_multiple_children {
            self.emit("_jsxs(");
        } else {
            self.emit("_jsx(");
        }

        // Tag name
        if crate::jsx::is_intrinsic_element(&tag) {
            self.emit("\"");
            self.emit(&tag);
            self.emit("\"");
        } else {
            self.emit(&tag);
        }

        self.emit(", ");

        // Props object
        self.emit("{");
        let mut first = true;

        for attr in &element.opening.attributes {
            match attr {
                JsxAttribute::Attribute { name, value, .. } => {
                    if !first { self.emit(", "); }
                    first = false;
                    self.emit_jsx_attr_name(name);
                    self.emit(": ");
                    match value {
                        Some(JsxAttrValue::String(s)) => {
                            self.emit("\"");
                            self.emit(s);
                            self.emit("\"");
                        }
                        Some(JsxAttrValue::Expr(expr)) => {
                            self.emit_expr(expr);
                        }
                        Some(JsxAttrValue::Element(el)) => {
                            self.emit_jsx_element(el);
                        }
                        Some(JsxAttrValue::Fragment(frag)) => {
                            self.emit_jsx_fragment(frag);
                        }
                        None => {
                            self.emit("true");
                        }
                    }
                }
                JsxAttribute::SpreadAttribute { argument, .. } => {
                    if !first { self.emit(", "); }
                    first = false;
                    self.emit("...");
                    self.emit_expr(argument);
                }
            }
        }

        // Children
        if !element.children.is_empty() {
            if !first { self.emit(", "); }
            self.emit("children: ");
            if has_multiple_children {
                self.emit("[");
                for (i, child) in element.children.iter().enumerate() {
                    if i > 0 { self.emit(", "); }
                    self.emit_jsx_child(child);
                }
                self.emit("]");
            } else if let Some(child) = element.children.first() {
                self.emit_jsx_child(child);
            }
        }

        self.emit("})");
    }

    #[cfg(feature = "jsx")]
    fn emit_jsx_fragment(&mut self, fragment: &JsxFragment) {
        let has_multiple_children = fragment.children.len() > 1;

        if has_multiple_children {
            self.emit("_jsxs(_Fragment, ");
        } else {
            self.emit("_jsx(_Fragment, ");
        }

        self.emit("{");
        if !fragment.children.is_empty() {
            self.emit("children: ");
            if has_multiple_children {
                self.emit("[");
                for (i, child) in fragment.children.iter().enumerate() {
                    if i > 0 { self.emit(", "); }
                    self.emit_jsx_child(child);
                }
                self.emit("]");
            } else if let Some(child) = fragment.children.first() {
                self.emit_jsx_child(child);
            }
        }
        self.emit("})");
    }

    #[cfg(feature = "jsx")]
    fn emit_jsx_child(&mut self, child: &JsxChild) {
        match child {
            JsxChild::Text(text) => {
                self.emit("\"");
                // Escape special chars
                for c in text.chars() {
                    match c {
                        '"' => self.emit("\\\""),
                        '\\' => self.emit("\\\\"),
                        '\n' => self.emit("\\n"),
                        _ => { self.output.push(c); }
                    }
                }
                self.emit("\"");
            }
            JsxChild::Element(el) => self.emit_jsx_element(el),
            JsxChild::Fragment(frag) => self.emit_jsx_fragment(frag),
            JsxChild::Expr(expr) => self.emit_expr(expr),
            JsxChild::Spread(expr) => {
                self.emit("...");
                self.emit_expr(expr);
            }
        }
    }

    #[cfg(feature = "jsx")]
    fn emit_jsx_attr_name(&mut self, name: &JsxAttrName) {
        match name {
            JsxAttrName::Ident(s) => self.emit(s),
            JsxAttrName::NamespacedName { namespace, name } => {
                self.emit("\"");
                self.emit(namespace);
                self.emit(":");
                self.emit(name);
                self.emit("\"");
            }
        }
    }

    #[cfg(feature = "jsx")]
    fn jsx_element_name_string(&self, name: &JsxElementName) -> String {
        match name {
            JsxElementName::Ident(s) => s.clone(),
            JsxElementName::MemberExpr(parts) => parts.join("."),
            JsxElementName::NamespacedName { namespace, name } => {
                format!("{}:{}", namespace, name)
            }
        }
    }

    fn emit_object_property(&mut self, prop: &Property) {
        if prop.shorthand {
            if let PropertyKey::Ident(name) = &prop.key {
                self.emit(name);
                return;
            }
        }

        match prop.kind {
            PropertyKind::Get => self.emit("get "),
            PropertyKind::Set => self.emit("set "),
            _ => {}
        }

        if prop.computed {
            self.emit("[");
        }
        self.emit_property_key(&prop.key);
        if prop.computed {
            self.emit("]");
        }

        match prop.kind {
            PropertyKind::Method => {
                // Method shorthand
                if let ExprKind::Function(func) = &prop.value.kind {
                    self.emit_params(&func.params);
                    self.emit_space();
                    self.emit_block(&func.body);
                } else {
                    self.emit(":");
                    self.emit_space();
                    self.emit_expr(&prop.value);
                }
            }
            PropertyKind::Get | PropertyKind::Set => {
                if let ExprKind::Function(func) = &prop.value.kind {
                    self.emit_params(&func.params);
                    self.emit_space();
                    self.emit_block(&func.body);
                }
            }
            PropertyKind::Init => {
                self.emit(":");
                self.emit_space();
                self.emit_expr(&prop.value);
            }
        }
    }

    fn emit_property_key(&mut self, key: &PropertyKey) {
        match key {
            PropertyKey::Ident(name) => self.emit(name),
            PropertyKey::String(s) => {
                self.emit("\"");
                self.emit(&escape_string(s));
                self.emit("\"");
            }
            PropertyKey::Number(n) => self.emit(&format_number(*n)),
            PropertyKey::Computed(expr) => {
                self.emit("[");
                self.emit_expr(expr);
                self.emit("]");
            }
        }
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{:.0}", n)
    } else {
        let s = format!("{}", n);
        // Use shorter exponential notation if beneficial
        let exp = format!("{:e}", n);
        if exp.len() < s.len() {
            exp
        } else {
            s
        }
    }
}

fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\0' => result.push_str("\\0"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

fn escape_template(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '`' => result.push_str("\\`"),
            '$' => result.push_str("\\$"),
            c => result.push(c),
        }
    }
    result
}

fn binary_op_info(op: BinaryOp) -> (u8, &'static str) {
    match op {
        BinaryOp::Or => (4, "||"),
        BinaryOp::And => (5, "&&"),
        BinaryOp::NullishCoalesce => (4, "??"),
        BinaryOp::BitOr => (6, "|"),
        BinaryOp::BitXor => (7, "^"),
        BinaryOp::BitAnd => (8, "&"),
        BinaryOp::Eq => (9, "=="),
        BinaryOp::NotEq => (9, "!="),
        BinaryOp::StrictEq => (9, "==="),
        BinaryOp::StrictNotEq => (9, "!=="),
        BinaryOp::Lt => (10, "<"),
        BinaryOp::LtEq => (10, "<="),
        BinaryOp::Gt => (10, ">"),
        BinaryOp::GtEq => (10, ">="),
        BinaryOp::In => (10, "in"),
        BinaryOp::Instanceof => (10, "instanceof"),
        BinaryOp::Shl => (11, "<<"),
        BinaryOp::Shr => (11, ">>"),
        BinaryOp::UShr => (11, ">>>"),
        BinaryOp::Add => (12, "+"),
        BinaryOp::Sub => (12, "-"),
        BinaryOp::Mul => (13, "*"),
        BinaryOp::Div => (13, "/"),
        BinaryOp::Mod => (13, "%"),
        BinaryOp::Pow => (14, "**"),
    }
}

fn is_right_associative(op: BinaryOp) -> bool {
    matches!(op, BinaryOp::Pow)
}

fn assign_op_str(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Assign => "=",
        AssignOp::AddAssign => "+=",
        AssignOp::SubAssign => "-=",
        AssignOp::MulAssign => "*=",
        AssignOp::DivAssign => "/=",
        AssignOp::ModAssign => "%=",
        AssignOp::PowAssign => "**=",
        AssignOp::ShlAssign => "<<=",
        AssignOp::ShrAssign => ">>=",
        AssignOp::UShrAssign => ">>>=",
        AssignOp::BitOrAssign => "|=",
        AssignOp::BitXorAssign => "^=",
        AssignOp::BitAndAssign => "&=",
        AssignOp::AndAssign => "&&=",
        AssignOp::OrAssign => "||=",
        AssignOp::NullishAssign => "??=",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Parser, ParserOptions};

    fn roundtrip(source: &str) -> String {
        let ast = Parser::new(source, ParserOptions::default()).parse().unwrap();
        Codegen::new(&ast, CodegenOptions::default()).generate()
    }

    #[test]
    fn test_variable_declaration() {
        let output = roundtrip("let x = 1;");
        assert!(output.contains("let x = 1"));
    }

    #[test]
    fn test_function_declaration() {
        let output = roundtrip("function foo(a, b) { return a + b; }");
        assert!(output.contains("function foo(a, b)"));
        assert!(output.contains("return a + b"));
    }

    #[test]
    fn test_minify() {
        let ast = Parser::new("let x = 1;\nlet y = 2;", ParserOptions::default()).parse().unwrap();
        let output = Codegen::new(&ast, CodegenOptions { minify: true, ..Default::default() }).generate();
        assert!(!output.contains('\n'));
    }
}
