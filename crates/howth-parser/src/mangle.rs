//! Variable name mangling for minification.
//!
//! Shortens local variable names (`myVariable` → `a`) by:
//! 1. Building a scope tree from the AST
//! 2. Assigning short names per scope (avoiding parent names and reserved words)
//! 3. Renaming identifiers in-place on the AST
//!
//! Property names, object keys, labels, and globals are never mangled.
//! Scopes containing `eval()` or `with` are skipped entirely.

use crate::ast::*;
use std::collections::{HashMap, HashSet};

/// Options for name mangling.
#[derive(Debug, Clone, Default)]
pub struct MangleOptions {
    /// User-specified names to never rename.
    pub reserved: HashSet<String>,
    /// Whether to mangle module-level (top-level) bindings.
    pub top_level: bool,
}

/// Mangle variable names in an AST in-place.
///
/// This is the main entry point. After calling this, all local variable names
/// in the AST will be shortened. The codegen can then emit the AST as-is.
pub fn mangle(ast: &mut Ast, options: &MangleOptions) {
    let mut ctx = MangleContext::new(options);

    // Phase 1: Collect — build scope tree, record all bindings
    ctx.collect_stmts(&ast.stmts, ctx.root_scope);

    // Phase 2: Assign — assign short names per scope
    ctx.assign_names();

    // Phase 3: Rename — walk AST, replace identifier names in-place
    rename_stmts(&mut ast.stmts, &ctx);
}

// =============================================================================
// Scope Tree
// =============================================================================

type ScopeId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    Module,
    Function,
    Block,
    Catch,
}

#[derive(Debug)]
struct Scope {
    parent: Option<ScopeId>,
    kind: ScopeKind,
    /// Original binding names declared in this scope.
    bindings: Vec<String>,
    /// Mapping from original name → mangled name (filled in phase 2).
    renames: HashMap<String, String>,
    /// If true, skip renaming in this scope (eval/with detected).
    has_eval: bool,
    children: Vec<ScopeId>,
}

impl Scope {
    fn new(kind: ScopeKind, parent: Option<ScopeId>) -> Self {
        Self {
            parent,
            kind,
            bindings: Vec::new(),
            renames: HashMap::new(),
            has_eval: false,
            children: Vec::new(),
        }
    }
}

struct MangleContext<'a> {
    options: &'a MangleOptions,
    scopes: Vec<Scope>,
    root_scope: ScopeId,
}

impl<'a> MangleContext<'a> {
    fn new(options: &'a MangleOptions) -> Self {
        let root = Scope::new(ScopeKind::Module, None);
        Self {
            options,
            scopes: vec![root],
            root_scope: 0,
        }
    }

    fn add_scope(&mut self, kind: ScopeKind, parent: ScopeId) -> ScopeId {
        let id = self.scopes.len();
        self.scopes.push(Scope::new(kind, Some(parent)));
        self.scopes[parent].children.push(id);
        id
    }

    /// Add a binding to a scope. For `var`, hoists to nearest function/module scope.
    fn add_binding(&mut self, name: &str, var_kind: Option<VarKind>, scope: ScopeId) {
        let target = match var_kind {
            Some(VarKind::Var) => self.hoist_target(scope),
            _ => scope,
        };
        let s = &mut self.scopes[target];
        if !s.bindings.contains(&name.to_string()) {
            s.bindings.push(name.to_string());
        }
    }

    /// Find the nearest function/module scope for var hoisting.
    fn hoist_target(&self, scope: ScopeId) -> ScopeId {
        let mut current = scope;
        loop {
            let kind = self.scopes[current].kind;
            if kind == ScopeKind::Function || kind == ScopeKind::Module {
                return current;
            }
            match self.scopes[current].parent {
                Some(p) => current = p,
                None => return current,
            }
        }
    }

    /// Mark a scope (and all ancestors) as having eval.
    fn mark_eval(&mut self, scope: ScopeId) {
        let mut current = Some(scope);
        while let Some(id) = current {
            self.scopes[id].has_eval = true;
            current = self.scopes[id].parent;
        }
    }

    // =========================================================================
    // Phase 1: Collect scopes and bindings
    // =========================================================================

    fn collect_stmts(&mut self, stmts: &[Stmt], scope: ScopeId) {
        for stmt in stmts {
            self.collect_stmt(stmt, scope);
        }
    }

    fn collect_stmt(&mut self, stmt: &Stmt, scope: ScopeId) {
        match &stmt.kind {
            StmtKind::Var { kind, decls } => {
                for decl in decls {
                    self.collect_binding(&decl.binding, Some(*kind), scope);
                    if let Some(init) = &decl.init {
                        self.collect_expr(init, scope);
                    }
                }
            }
            StmtKind::Function(f) => {
                // Function name is declared in the enclosing scope
                if let Some(name) = &f.name {
                    self.add_binding(name, None, scope);
                }
                self.collect_function(f, scope);
            }
            StmtKind::Class(c) => {
                if let Some(name) = &c.name {
                    self.add_binding(name, None, scope);
                }
                self.collect_class(c, scope);
            }
            StmtKind::Block(stmts) => {
                let block_scope = self.add_scope(ScopeKind::Block, scope);
                self.collect_stmts(stmts, block_scope);
            }
            StmtKind::If { test, consequent, alternate } => {
                self.collect_expr(test, scope);
                self.collect_stmt(consequent, scope);
                if let Some(alt) = alternate {
                    self.collect_stmt(alt, scope);
                }
            }
            StmtKind::Switch { discriminant, cases } => {
                self.collect_expr(discriminant, scope);
                // Switch body gets its own block scope for let/const
                let switch_scope = self.add_scope(ScopeKind::Block, scope);
                for case in cases {
                    if let Some(test) = &case.test {
                        self.collect_expr(test, switch_scope);
                    }
                    self.collect_stmts(&case.consequent, switch_scope);
                }
            }
            StmtKind::For { init, test, update, body } => {
                // For-loop gets an implicit block scope for `let`/`const` in init
                let for_scope = self.add_scope(ScopeKind::Block, scope);
                if let Some(init) = init {
                    match init {
                        ForInit::Var { kind, decls } => {
                            for decl in decls {
                                self.collect_binding(&decl.binding, Some(*kind), for_scope);
                                if let Some(init_expr) = &decl.init {
                                    self.collect_expr(init_expr, for_scope);
                                }
                            }
                        }
                        ForInit::Expr(e) => self.collect_expr(e, for_scope),
                    }
                }
                if let Some(test) = test {
                    self.collect_expr(test, for_scope);
                }
                if let Some(update) = update {
                    self.collect_expr(update, for_scope);
                }
                self.collect_stmt(body, for_scope);
            }
            StmtKind::ForIn { left, right, body } => {
                let for_scope = self.add_scope(ScopeKind::Block, scope);
                match left {
                    ForInit::Var { kind, decls } => {
                        for decl in decls {
                            self.collect_binding(&decl.binding, Some(*kind), for_scope);
                        }
                    }
                    ForInit::Expr(e) => self.collect_expr(e, for_scope),
                }
                self.collect_expr(right, for_scope);
                self.collect_stmt(body, for_scope);
            }
            StmtKind::ForOf { left, right, body, .. } => {
                let for_scope = self.add_scope(ScopeKind::Block, scope);
                match left {
                    ForInit::Var { kind, decls } => {
                        for decl in decls {
                            self.collect_binding(&decl.binding, Some(*kind), for_scope);
                        }
                    }
                    ForInit::Expr(e) => self.collect_expr(e, for_scope),
                }
                self.collect_expr(right, for_scope);
                self.collect_stmt(body, for_scope);
            }
            StmtKind::While { test, body } => {
                self.collect_expr(test, scope);
                self.collect_stmt(body, scope);
            }
            StmtKind::DoWhile { body, test } => {
                self.collect_stmt(body, scope);
                self.collect_expr(test, scope);
            }
            StmtKind::Return { arg } => {
                if let Some(arg) = arg {
                    self.collect_expr(arg, scope);
                }
            }
            StmtKind::Throw { arg } => {
                self.collect_expr(arg, scope);
            }
            StmtKind::Try { block, handler, finalizer } => {
                let try_scope = self.add_scope(ScopeKind::Block, scope);
                self.collect_stmts(block, try_scope);
                if let Some(catch) = handler {
                    let catch_scope = self.add_scope(ScopeKind::Catch, scope);
                    if let Some(param) = &catch.param {
                        self.collect_binding(param, None, catch_scope);
                    }
                    self.collect_stmts(&catch.body, catch_scope);
                }
                if let Some(fin) = finalizer {
                    let fin_scope = self.add_scope(ScopeKind::Block, scope);
                    self.collect_stmts(fin, fin_scope);
                }
            }
            StmtKind::Labeled { body, .. } => {
                // Labels are NOT mangled
                self.collect_stmt(body, scope);
            }
            StmtKind::With { object, body } => {
                // `with` — bail out entire scope chain
                self.collect_expr(object, scope);
                self.mark_eval(scope);
                self.collect_stmt(body, scope);
            }
            StmtKind::Expr(e) => self.collect_expr(e, scope),
            StmtKind::Import(import_decl) => {
                // Import bindings are module-level
                for spec in &import_decl.specifiers {
                    match spec {
                        ImportSpecifier::Default { local, .. }
                        | ImportSpecifier::Namespace { local, .. } => {
                            self.add_binding(local, None, scope);
                        }
                        ImportSpecifier::Named { local, .. } => {
                            self.add_binding(local, None, scope);
                        }
                    }
                }
            }
            StmtKind::Export(export) => {
                self.collect_export(export, scope);
            }
            StmtKind::Empty | StmtKind::Debugger => {}
            StmtKind::Break { .. } | StmtKind::Continue { .. } => {}

            #[cfg(feature = "typescript")]
            StmtKind::TsTypeAlias(_)
            | StmtKind::TsInterface(_)
            | StmtKind::TsEnum(_)
            | StmtKind::TsNamespace(_) => {}
            #[cfg(feature = "typescript")]
            StmtKind::TsDeclare(inner) => self.collect_stmt(inner, scope),
        }
    }

    fn collect_export(&mut self, export: &ExportDecl, scope: ScopeId) {
        match export {
            ExportDecl::Default { expr, .. } => {
                self.collect_expr(expr, scope);
            }
            ExportDecl::Decl { decl, .. } => {
                self.collect_stmt(decl, scope);
            }
            ExportDecl::Named { .. } | ExportDecl::All { .. } => {}
        }
    }

    fn collect_function(&mut self, f: &Function, parent_scope: ScopeId) {
        let fn_scope = self.add_scope(ScopeKind::Function, parent_scope);
        // Function expression name is visible inside its own scope (for self-reference)
        // but function declaration name was already added to the parent scope
        // For function expressions with names, add the name to the fn scope too
        for param in &f.params {
            self.collect_binding(&param.binding, None, fn_scope);
            if let Some(default) = &param.default {
                self.collect_expr(default, fn_scope);
            }
        }
        self.collect_stmts(&f.body, fn_scope);
    }

    fn collect_class(&mut self, c: &Class, parent_scope: ScopeId) {
        if let Some(super_class) = &c.super_class {
            self.collect_expr(super_class, parent_scope);
        }
        for member in &c.body {
            match &member.kind {
                ClassMemberKind::Method { value, .. } => {
                    self.collect_function(value, parent_scope);
                }
                ClassMemberKind::Property { value, .. } => {
                    if let Some(v) = value {
                        self.collect_expr(v, parent_scope);
                    }
                }
                ClassMemberKind::StaticBlock(stmts) => {
                    let block_scope = self.add_scope(ScopeKind::Function, parent_scope);
                    self.collect_stmts(stmts, block_scope);
                }
                ClassMemberKind::Empty => {}
            }
        }
    }

    fn collect_arrow(&mut self, arrow: &ArrowFunction, parent_scope: ScopeId) {
        let fn_scope = self.add_scope(ScopeKind::Function, parent_scope);
        for param in &arrow.params {
            self.collect_binding(&param.binding, None, fn_scope);
            if let Some(default) = &param.default {
                self.collect_expr(default, fn_scope);
            }
        }
        match &arrow.body {
            ArrowBody::Expr(e) => self.collect_expr(e, fn_scope),
            ArrowBody::Block(stmts) => self.collect_stmts(stmts, fn_scope),
        }
    }

    fn collect_binding(&mut self, binding: &Binding, var_kind: Option<VarKind>, scope: ScopeId) {
        match &binding.kind {
            BindingKind::Ident { name, .. } => {
                self.add_binding(name, var_kind, scope);
            }
            BindingKind::Array { elements, .. } => {
                for elem in elements.iter().flatten() {
                    self.collect_binding(&elem.binding, var_kind, scope);
                    if let Some(default) = &elem.default {
                        self.collect_expr(default, scope);
                    }
                }
            }
            BindingKind::Object { properties, .. } => {
                for prop in properties {
                    if let PropertyKey::Computed(e) = &prop.key {
                        self.collect_expr(e, scope);
                    }
                    self.collect_binding(&prop.value, var_kind, scope);
                    if let Some(default) = &prop.default {
                        self.collect_expr(default, scope);
                    }
                }
            }
        }
    }

    fn collect_expr(&mut self, expr: &Expr, scope: ScopeId) {
        match &expr.kind {
            ExprKind::Function(f) => {
                // Function expression — name is in its own scope
                let fn_scope = self.add_scope(ScopeKind::Function, scope);
                if let Some(name) = &f.name {
                    // Function expression name only visible inside
                    self.add_binding(name, None, fn_scope);
                }
                for param in &f.params {
                    self.collect_binding(&param.binding, None, fn_scope);
                    if let Some(default) = &param.default {
                        self.collect_expr(default, fn_scope);
                    }
                }
                self.collect_stmts(&f.body, fn_scope);
            }
            ExprKind::Arrow(arrow) => {
                self.collect_arrow(arrow, scope);
            }
            ExprKind::Class(c) => {
                self.collect_class(c, scope);
            }
            ExprKind::Call { callee, args } => {
                // Detect eval()
                if let ExprKind::Ident(name) = &callee.kind {
                    if name == "eval" {
                        self.mark_eval(scope);
                    }
                }
                self.collect_expr(callee, scope);
                for arg in args {
                    self.collect_expr(arg, scope);
                }
            }
            ExprKind::Array(elems) => {
                for elem in elems.iter().flatten() {
                    self.collect_expr(elem, scope);
                }
            }
            ExprKind::Object(props) => {
                for prop in props {
                    if let PropertyKey::Computed(e) = &prop.key {
                        self.collect_expr(e, scope);
                    }
                    self.collect_expr(&prop.value, scope);
                }
            }
            ExprKind::Unary { arg, .. } => self.collect_expr(arg, scope),
            ExprKind::Binary { left, right, .. } => {
                self.collect_expr(left, scope);
                self.collect_expr(right, scope);
            }
            ExprKind::Assign { left, right, .. } => {
                self.collect_expr(left, scope);
                self.collect_expr(right, scope);
            }
            ExprKind::Update { arg, .. } => self.collect_expr(arg, scope),
            ExprKind::Conditional { test, consequent, alternate } => {
                self.collect_expr(test, scope);
                self.collect_expr(consequent, scope);
                self.collect_expr(alternate, scope);
            }
            ExprKind::Sequence(exprs) => {
                for e in exprs {
                    self.collect_expr(e, scope);
                }
            }
            ExprKind::Member { object, property, computed } => {
                self.collect_expr(object, scope);
                if *computed {
                    self.collect_expr(property, scope);
                }
            }
            ExprKind::OptionalMember { object, property, computed } => {
                self.collect_expr(object, scope);
                if *computed {
                    self.collect_expr(property, scope);
                }
            }
            ExprKind::OptionalCall { callee, args } => {
                self.collect_expr(callee, scope);
                for arg in args {
                    self.collect_expr(arg, scope);
                }
            }
            ExprKind::New { callee, args } => {
                self.collect_expr(callee, scope);
                for arg in args {
                    self.collect_expr(arg, scope);
                }
            }
            ExprKind::TaggedTemplate { tag, quasi } => {
                self.collect_expr(tag, scope);
                self.collect_expr(quasi, scope);
            }
            ExprKind::Template { exprs, .. } => {
                for e in exprs {
                    self.collect_expr(e, scope);
                }
            }
            ExprKind::Spread(e) => self.collect_expr(e, scope),
            ExprKind::Yield { arg, .. } => {
                if let Some(arg) = arg {
                    self.collect_expr(arg, scope);
                }
            }
            ExprKind::Await(e) => self.collect_expr(e, scope),
            ExprKind::Import(e) => self.collect_expr(e, scope),
            // Leaves — no children to collect
            ExprKind::Ident(_)
            | ExprKind::Null
            | ExprKind::Bool(_)
            | ExprKind::Number(_)
            | ExprKind::BigInt(_)
            | ExprKind::String(_)
            | ExprKind::Regex { .. }
            | ExprKind::TemplateNoSub(_)
            | ExprKind::This
            | ExprKind::Super
            | ExprKind::MetaProperty { .. } => {}

            #[cfg(feature = "jsx")]
            ExprKind::JsxElement(el) => self.collect_jsx_element(el, scope),
            #[cfg(feature = "jsx")]
            ExprKind::JsxFragment(frag) => self.collect_jsx_fragment(frag, scope),

            #[cfg(feature = "typescript")]
            ExprKind::TsAs { expr, .. }
            | ExprKind::TsSatisfies { expr, .. }
            | ExprKind::TsNonNull(expr)
            | ExprKind::TsTypeAssertion { expr, .. } => {
                self.collect_expr(expr, scope);
            }
        }
    }

    #[cfg(feature = "jsx")]
    fn collect_jsx_element(&mut self, el: &JsxElement, scope: ScopeId) {
        for attr in &el.opening.attributes {
            match attr {
                JsxAttribute::Attribute { value: Some(JsxAttrValue::Expr(e)), .. } => {
                    self.collect_expr(e, scope);
                }
                JsxAttribute::SpreadAttribute { argument, .. } => {
                    self.collect_expr(argument, scope);
                }
                _ => {}
            }
        }
        for child in &el.children {
            self.collect_jsx_child(child, scope);
        }
    }

    #[cfg(feature = "jsx")]
    fn collect_jsx_fragment(&mut self, frag: &JsxFragment, scope: ScopeId) {
        for child in &frag.children {
            self.collect_jsx_child(child, scope);
        }
    }

    #[cfg(feature = "jsx")]
    fn collect_jsx_child(&mut self, child: &JsxChild, scope: ScopeId) {
        match child {
            JsxChild::Expr(e) | JsxChild::Spread(e) => self.collect_expr(e, scope),
            JsxChild::Element(el) => self.collect_jsx_element(el, scope),
            JsxChild::Fragment(frag) => self.collect_jsx_fragment(frag, scope),
            JsxChild::Text(_) => {}
        }
    }

    // =========================================================================
    // Phase 2: Assign short names
    // =========================================================================

    fn assign_names(&mut self) {
        // Process scopes top-down so parent names are assigned first
        let scope_ids: Vec<ScopeId> = (0..self.scopes.len()).collect();
        for id in scope_ids {
            self.assign_scope_names(id);
        }
    }

    fn assign_scope_names(&mut self, scope_id: ScopeId) {
        // Skip module scope if top_level is false
        if scope_id == self.root_scope && !self.options.top_level {
            return;
        }

        // Skip if eval detected
        if self.scopes[scope_id].has_eval {
            return;
        }

        // Collect names that are off-limits (from parent scopes)
        let used_names = self.collect_ancestor_names(scope_id);

        let bindings = self.scopes[scope_id].bindings.clone();
        let mut gen = NameGenerator::new();

        for name in &bindings {
            // Skip reserved names
            if self.options.reserved.contains(name) {
                continue;
            }

            // Generate the next short name that's not already used
            loop {
                let candidate = gen.next();
                if !used_names.contains(&candidate)
                    && !is_js_reserved(&candidate)
                    && !self.options.reserved.contains(&candidate)
                    && !self.scope_has_binding_named(scope_id, &candidate, name)
                {
                    self.scopes[scope_id]
                        .renames
                        .insert(name.clone(), candidate);
                    break;
                }
            }
        }
    }

    /// Check if this scope has another binding with the candidate name
    /// (different from the one we're currently renaming).
    fn scope_has_binding_named(&self, scope_id: ScopeId, candidate: &str, excluding: &str) -> bool {
        self.scopes[scope_id]
            .bindings
            .iter()
            .any(|b| b != excluding && b == candidate)
    }

    /// Collect all mangled names already assigned in ancestor scopes.
    fn collect_ancestor_names(&self, scope_id: ScopeId) -> HashSet<String> {
        let mut names = HashSet::new();
        let mut current = self.scopes[scope_id].parent;
        while let Some(id) = current {
            for mangled in self.scopes[id].renames.values() {
                names.insert(mangled.clone());
            }
            // Also include original names that weren't renamed (reserved/module-level)
            for orig in &self.scopes[id].bindings {
                if !self.scopes[id].renames.contains_key(orig) {
                    names.insert(orig.clone());
                }
            }
            current = self.scopes[id].parent;
        }
        names
    }

    // =========================================================================
    // Phase 3 helpers: look up the mangled name for an identifier
    // =========================================================================

    /// Look up the mangled name for an identifier, walking up the scope chain.
    fn resolve_name(&self, original: &str, scope_id: ScopeId) -> Option<String> {
        let mut current = Some(scope_id);
        while let Some(id) = current {
            if let Some(mangled) = self.scopes[id].renames.get(original) {
                return Some(mangled.clone());
            }
            // Check if it's a binding in this scope (but not renamed, e.g. reserved)
            if self.scopes[id].bindings.contains(&original.to_string()) {
                return None; // Declared but not renamed
            }
            current = self.scopes[id].parent;
        }
        None // Global — not declared in any scope
    }
}

// =============================================================================
// Name Generator
// =============================================================================

/// Generates short identifier names: a, b, ..., z, A, ..., Z, _, $, aa, ab, ...
struct NameGenerator {
    counter: usize,
}

/// Characters used for the first position of generated names.
const FIRST_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_$";

/// Characters used for subsequent positions (includes digits).
const REST_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_$";

impl NameGenerator {
    fn new() -> Self {
        Self { counter: 0 }
    }

    fn next(&mut self) -> String {
        let name = encode_name(self.counter);
        self.counter += 1;
        name
    }
}

fn encode_name(mut n: usize) -> String {
    let first_len = FIRST_CHARS.len(); // 54
    let rest_len = REST_CHARS.len(); // 64

    // First character
    let first_idx = n % first_len;
    n /= first_len;

    if n == 0 {
        return String::from(FIRST_CHARS[first_idx] as char);
    }

    let mut name = String::with_capacity(4);
    name.push(FIRST_CHARS[first_idx] as char);

    // Subsequent characters
    while n > 0 {
        n -= 1; // Make it 0-based for this digit
        let idx = n % rest_len;
        n /= rest_len;
        name.push(REST_CHARS[idx] as char);
    }

    name
}

// =============================================================================
// JS Reserved Words
// =============================================================================

fn is_js_reserved(name: &str) -> bool {
    matches!(
        name,
        "do" | "if" | "in" | "for" | "let" | "new" | "try" | "var" | "case" | "else" | "enum"
            | "eval" | "null" | "this" | "true" | "void" | "with" | "await" | "break"
            | "catch" | "class" | "const" | "false" | "super" | "throw" | "while" | "yield"
            | "delete" | "export" | "import" | "return" | "switch" | "typeof"
            | "default" | "extends" | "finally" | "continue" | "debugger" | "function"
            | "arguments" | "instanceof" | "of"
    )
}

// =============================================================================
// Phase 3: Rename AST in-place
// =============================================================================

/// The rename phase walks the AST with a scope stack, renaming identifiers.
/// We reconstruct the scope structure implicitly by matching the same traversal
/// pattern used in the collect phase.
fn rename_stmts(stmts: &mut [Stmt], ctx: &MangleContext) {
    let mut renamer = Renamer::new(ctx);
    renamer.rename_stmts(stmts);
}

struct Renamer<'a> {
    ctx: &'a MangleContext<'a>,
    /// Stack of scope IDs matching our traversal.
    scope_stack: Vec<ScopeId>,
    /// Counter for child scopes created within each scope.
    /// Used to track which scope we're entering next.
    child_counters: Vec<usize>,
}

impl<'a> Renamer<'a> {
    fn new(ctx: &'a MangleContext) -> Self {
        Self {
            ctx,
            scope_stack: vec![ctx.root_scope],
            child_counters: vec![0],
        }
    }

    fn current_scope(&self) -> ScopeId {
        *self.scope_stack.last().unwrap()
    }

    /// Enter the next child scope of the current scope.
    fn enter_scope(&mut self) {
        let parent = self.current_scope();
        let child_idx = *self.child_counters.last().unwrap();
        let child_scope = self.ctx.scopes[parent].children[child_idx];
        // Increment the parent's child counter
        *self.child_counters.last_mut().unwrap() = child_idx + 1;
        self.scope_stack.push(child_scope);
        self.child_counters.push(0);
    }

    fn leave_scope(&mut self) {
        self.scope_stack.pop();
        self.child_counters.pop();
    }

    /// Resolve an identifier to its mangled name.
    fn resolve(&self, name: &str) -> Option<String> {
        self.ctx.resolve_name(name, self.current_scope())
    }

    fn rename_ident(&self, name: &mut String) {
        if let Some(mangled) = self.resolve(name) {
            *name = mangled;
        }
    }

    fn rename_stmts(&mut self, stmts: &mut [Stmt]) {
        for stmt in stmts.iter_mut() {
            self.rename_stmt(stmt);
        }
    }

    fn rename_stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.kind {
            StmtKind::Var { decls, .. } => {
                for decl in decls {
                    self.rename_binding(&mut decl.binding);
                    if let Some(init) = &mut decl.init {
                        self.rename_expr(init);
                    }
                }
            }
            StmtKind::Function(f) => {
                if let Some(name) = &mut f.name {
                    self.rename_ident(name);
                }
                self.rename_function(f);
            }
            StmtKind::Class(c) => {
                if let Some(name) = &mut c.name {
                    self.rename_ident(name);
                }
                self.rename_class(c);
            }
            StmtKind::Block(stmts) => {
                self.enter_scope();
                self.rename_stmts(stmts);
                self.leave_scope();
            }
            StmtKind::If { test, consequent, alternate } => {
                self.rename_expr(test);
                self.rename_stmt(consequent);
                if let Some(alt) = alternate {
                    self.rename_stmt(alt);
                }
            }
            StmtKind::Switch { discriminant, cases } => {
                self.rename_expr(discriminant);
                self.enter_scope();
                for case in cases {
                    if let Some(test) = &mut case.test {
                        self.rename_expr(test);
                    }
                    self.rename_stmts(&mut case.consequent);
                }
                self.leave_scope();
            }
            StmtKind::For { init, test, update, body } => {
                self.enter_scope();
                if let Some(init) = init {
                    match init {
                        ForInit::Var { decls, .. } => {
                            for decl in decls {
                                self.rename_binding(&mut decl.binding);
                                if let Some(init_expr) = &mut decl.init {
                                    self.rename_expr(init_expr);
                                }
                            }
                        }
                        ForInit::Expr(e) => self.rename_expr(e),
                    }
                }
                if let Some(test) = test {
                    self.rename_expr(test);
                }
                if let Some(update) = update {
                    self.rename_expr(update);
                }
                self.rename_stmt(body);
                self.leave_scope();
            }
            StmtKind::ForIn { left, right, body } => {
                self.enter_scope();
                match left {
                    ForInit::Var { decls, .. } => {
                        for decl in decls {
                            self.rename_binding(&mut decl.binding);
                        }
                    }
                    ForInit::Expr(e) => self.rename_expr(e),
                }
                self.rename_expr(right);
                self.rename_stmt(body);
                self.leave_scope();
            }
            StmtKind::ForOf { left, right, body, .. } => {
                self.enter_scope();
                match left {
                    ForInit::Var { decls, .. } => {
                        for decl in decls {
                            self.rename_binding(&mut decl.binding);
                        }
                    }
                    ForInit::Expr(e) => self.rename_expr(e),
                }
                self.rename_expr(right);
                self.rename_stmt(body);
                self.leave_scope();
            }
            StmtKind::While { test, body } => {
                self.rename_expr(test);
                self.rename_stmt(body);
            }
            StmtKind::DoWhile { body, test } => {
                self.rename_stmt(body);
                self.rename_expr(test);
            }
            StmtKind::Return { arg } => {
                if let Some(arg) = arg {
                    self.rename_expr(arg);
                }
            }
            StmtKind::Throw { arg } => {
                self.rename_expr(arg);
            }
            StmtKind::Try { block, handler, finalizer } => {
                self.enter_scope(); // try block scope
                self.rename_stmts(block);
                self.leave_scope();
                if let Some(catch) = handler {
                    self.enter_scope(); // catch scope
                    if let Some(param) = &mut catch.param {
                        self.rename_binding(param);
                    }
                    self.rename_stmts(&mut catch.body);
                    self.leave_scope();
                }
                if let Some(fin) = finalizer {
                    self.enter_scope(); // finalizer scope
                    self.rename_stmts(fin);
                    self.leave_scope();
                }
            }
            StmtKind::Labeled { body, .. } => {
                // Don't rename labels
                self.rename_stmt(body);
            }
            StmtKind::With { object, body } => {
                self.rename_expr(object);
                self.rename_stmt(body);
            }
            StmtKind::Expr(e) => self.rename_expr(e),
            StmtKind::Import(import_decl) => {
                for spec in &mut import_decl.specifiers {
                    match spec {
                        ImportSpecifier::Default { local, .. }
                        | ImportSpecifier::Namespace { local, .. } => {
                            self.rename_ident(local);
                        }
                        ImportSpecifier::Named { local, .. } => {
                            // `imported` stays the same (it's the external name)
                            self.rename_ident(local);
                        }
                    }
                }
            }
            StmtKind::Export(export) => {
                self.rename_export(export);
            }
            StmtKind::Empty | StmtKind::Debugger => {}
            StmtKind::Break { .. } | StmtKind::Continue { .. } => {}

            #[cfg(feature = "typescript")]
            StmtKind::TsTypeAlias(_)
            | StmtKind::TsInterface(_)
            | StmtKind::TsEnum(_)
            | StmtKind::TsNamespace(_) => {}
            #[cfg(feature = "typescript")]
            StmtKind::TsDeclare(inner) => self.rename_stmt(inner),
        }
    }

    fn rename_export(&mut self, export: &mut ExportDecl) {
        match export {
            ExportDecl::Default { expr, .. } => {
                self.rename_expr(expr);
            }
            ExportDecl::Decl { decl, .. } => {
                self.rename_stmt(decl);
            }
            ExportDecl::Named { specifiers, .. } => {
                for spec in specifiers {
                    // local refers to the binding — rename it
                    if let Some(mangled) = self.resolve(&spec.local) {
                        spec.local = mangled;
                    }
                    // exported stays the same (it's the public API name)
                }
            }
            ExportDecl::All { .. } => {}
        }
    }

    fn rename_function(&mut self, f: &mut Function) {
        self.enter_scope();
        for param in &mut f.params {
            self.rename_binding(&mut param.binding);
            if let Some(default) = &mut param.default {
                self.rename_expr(default);
            }
        }
        self.rename_stmts(&mut f.body);
        self.leave_scope();
    }

    fn rename_class(&mut self, c: &mut Class) {
        if let Some(super_class) = &mut c.super_class {
            self.rename_expr(super_class);
        }
        for member in &mut c.body {
            match &mut member.kind {
                ClassMemberKind::Method { value, .. } => {
                    self.rename_function(value);
                }
                ClassMemberKind::Property { value, .. } => {
                    if let Some(v) = value {
                        self.rename_expr(v);
                    }
                }
                ClassMemberKind::StaticBlock(stmts) => {
                    self.enter_scope();
                    self.rename_stmts(stmts);
                    self.leave_scope();
                }
                ClassMemberKind::Empty => {}
            }
        }
    }

    fn rename_arrow(&mut self, arrow: &mut ArrowFunction) {
        self.enter_scope();
        for param in &mut arrow.params {
            self.rename_binding(&mut param.binding);
            if let Some(default) = &mut param.default {
                self.rename_expr(default);
            }
        }
        match &mut arrow.body {
            ArrowBody::Expr(e) => self.rename_expr(e),
            ArrowBody::Block(stmts) => self.rename_stmts(stmts),
        }
        self.leave_scope();
    }

    fn rename_binding(&mut self, binding: &mut Binding) {
        match &mut binding.kind {
            BindingKind::Ident { name, .. } => {
                self.rename_ident(name);
            }
            BindingKind::Array { elements, .. } => {
                for elem in elements.iter_mut().flatten() {
                    self.rename_binding(&mut elem.binding);
                    if let Some(default) = &mut elem.default {
                        self.rename_expr(default);
                    }
                }
            }
            BindingKind::Object { properties, .. } => {
                for prop in properties {
                    if let PropertyKey::Computed(e) = &mut prop.key {
                        self.rename_expr(e);
                    }
                    if prop.shorthand {
                        // Shorthand: `{ foo }` → need to expand to `{ foo: a }`
                        // The key stays as the original name, value gets renamed
                        let original_name = match &prop.value.kind {
                            BindingKind::Ident { name, .. } => name.clone(),
                            _ => {
                                // Non-ident shorthand — just rename normally
                                self.rename_binding(&mut prop.value);
                                continue;
                            }
                        };
                        if let Some(mangled) = self.resolve(&original_name) {
                            // Expand shorthand: set key to original, rename value
                            prop.shorthand = false;
                            prop.key = PropertyKey::Ident(original_name);
                            if let BindingKind::Ident { name, .. } = &mut prop.value.kind {
                                *name = mangled;
                            }
                        }
                    } else {
                        // Non-shorthand: key stays, value is renamed
                        self.rename_binding(&mut prop.value);
                    }
                    if let Some(default) = &mut prop.default {
                        self.rename_expr(default);
                    }
                }
            }
        }
    }

    fn rename_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Ident(name) => {
                self.rename_ident(name);
            }
            ExprKind::Function(f) => {
                // Function expression creates its own scope
                self.enter_scope();
                if let Some(name) = &mut f.name {
                    self.rename_ident(name);
                }
                for param in &mut f.params {
                    self.rename_binding(&mut param.binding);
                    if let Some(default) = &mut param.default {
                        self.rename_expr(default);
                    }
                }
                self.rename_stmts(&mut f.body);
                self.leave_scope();
            }
            ExprKind::Arrow(arrow) => {
                self.rename_arrow(arrow);
            }
            ExprKind::Class(c) => {
                self.rename_class(c);
            }
            ExprKind::Array(elems) => {
                for elem in elems.iter_mut().flatten() {
                    self.rename_expr(elem);
                }
            }
            ExprKind::Object(props) => {
                for prop in props {
                    // Property keys that are identifiers are NOT renamed (they're property names)
                    if let PropertyKey::Computed(e) = &mut prop.key {
                        self.rename_expr(e);
                    }
                    if prop.shorthand {
                        // Shorthand property: `{ foo }` → `{ foo: a }`
                        if let ExprKind::Ident(name) = &prop.value.kind {
                            let original = name.clone();
                            if let Some(mangled) = self.resolve(&original) {
                                prop.shorthand = false;
                                prop.key = PropertyKey::Ident(original);
                                if let ExprKind::Ident(ref mut val_name) = &mut prop.value.kind {
                                    *val_name = mangled;
                                }
                            }
                        } else {
                            self.rename_expr(&mut prop.value);
                        }
                    } else {
                        self.rename_expr(&mut prop.value);
                    }
                }
            }
            ExprKind::Unary { arg, .. } => self.rename_expr(arg),
            ExprKind::Binary { left, right, .. } => {
                self.rename_expr(left);
                self.rename_expr(right);
            }
            ExprKind::Assign { left, right, .. } => {
                self.rename_expr(left);
                self.rename_expr(right);
            }
            ExprKind::Update { arg, .. } => self.rename_expr(arg),
            ExprKind::Conditional { test, consequent, alternate } => {
                self.rename_expr(test);
                self.rename_expr(consequent);
                self.rename_expr(alternate);
            }
            ExprKind::Sequence(exprs) => {
                for e in exprs {
                    self.rename_expr(e);
                }
            }
            ExprKind::Member { object, property, computed } => {
                self.rename_expr(object);
                // Only rename computed properties — `obj.foo` stays `obj.foo`
                if *computed {
                    self.rename_expr(property);
                }
            }
            ExprKind::OptionalMember { object, property, computed } => {
                self.rename_expr(object);
                if *computed {
                    self.rename_expr(property);
                }
            }
            ExprKind::Call { callee, args } => {
                self.rename_expr(callee);
                for arg in args {
                    self.rename_expr(arg);
                }
            }
            ExprKind::OptionalCall { callee, args } => {
                self.rename_expr(callee);
                for arg in args {
                    self.rename_expr(arg);
                }
            }
            ExprKind::New { callee, args } => {
                self.rename_expr(callee);
                for arg in args {
                    self.rename_expr(arg);
                }
            }
            ExprKind::TaggedTemplate { tag, quasi } => {
                self.rename_expr(tag);
                self.rename_expr(quasi);
            }
            ExprKind::Template { exprs, .. } => {
                for e in exprs {
                    self.rename_expr(e);
                }
            }
            ExprKind::Spread(e) => self.rename_expr(e),
            ExprKind::Yield { arg, .. } => {
                if let Some(arg) = arg {
                    self.rename_expr(arg);
                }
            }
            ExprKind::Await(e) => self.rename_expr(e),
            ExprKind::Import(e) => self.rename_expr(e),
            // Leaves — nothing to rename
            ExprKind::Null
            | ExprKind::Bool(_)
            | ExprKind::Number(_)
            | ExprKind::BigInt(_)
            | ExprKind::String(_)
            | ExprKind::Regex { .. }
            | ExprKind::TemplateNoSub(_)
            | ExprKind::This
            | ExprKind::Super
            | ExprKind::MetaProperty { .. } => {}

            #[cfg(feature = "jsx")]
            ExprKind::JsxElement(el) => self.rename_jsx_element(el),
            #[cfg(feature = "jsx")]
            ExprKind::JsxFragment(frag) => self.rename_jsx_fragment(frag),

            #[cfg(feature = "typescript")]
            ExprKind::TsAs { expr, .. }
            | ExprKind::TsSatisfies { expr, .. }
            | ExprKind::TsNonNull(expr)
            | ExprKind::TsTypeAssertion { expr, .. } => {
                self.rename_expr(expr);
            }
        }
    }

    #[cfg(feature = "jsx")]
    fn rename_jsx_element(&mut self, el: &mut JsxElement) {
        for attr in &mut el.opening.attributes {
            match attr {
                JsxAttribute::Attribute { value: Some(JsxAttrValue::Expr(e)), .. } => {
                    self.rename_expr(e);
                }
                JsxAttribute::SpreadAttribute { argument, .. } => {
                    self.rename_expr(argument);
                }
                _ => {}
            }
        }
        for child in &mut el.children {
            self.rename_jsx_child(child);
        }
    }

    #[cfg(feature = "jsx")]
    fn rename_jsx_fragment(&mut self, frag: &mut JsxFragment) {
        for child in &mut frag.children {
            self.rename_jsx_child(child);
        }
    }

    #[cfg(feature = "jsx")]
    fn rename_jsx_child(&mut self, child: &mut JsxChild) {
        match child {
            JsxChild::Expr(e) | JsxChild::Spread(e) => self.rename_expr(e),
            JsxChild::Element(el) => self.rename_jsx_element(el),
            JsxChild::Fragment(frag) => self.rename_jsx_fragment(frag),
            JsxChild::Text(_) => {}
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Parser, ParserOptions};

    fn parse_and_mangle(source: &str, opts: &MangleOptions) -> String {
        let mut ast = Parser::new(source, ParserOptions::default()).parse().unwrap();
        mangle(&mut ast, opts);
        crate::Codegen::new(&ast, crate::CodegenOptions::default()).generate()
    }

    fn parse_and_mangle_top(source: &str) -> String {
        let opts = MangleOptions {
            top_level: true,
            ..Default::default()
        };
        parse_and_mangle(source, &opts)
    }

    #[test]
    fn test_basic_variable_renaming() {
        let result = parse_and_mangle_top("let myVariable = 1; console.log(myVariable);");
        assert!(result.contains("let a"));
        assert!(result.contains("console.log(a)"));
        assert!(!result.contains("myVariable"));
    }

    #[test]
    fn test_function_params() {
        let result = parse_and_mangle(
            "function foo(longParam, anotherParam) { return longParam + anotherParam; }",
            &MangleOptions::default(),
        );
        // Function name is module-level — not mangled by default (top_level: false)
        assert!(result.contains("function foo"));
        // But params should be mangled
        assert!(!result.contains("longParam"));
        assert!(!result.contains("anotherParam"));
    }

    #[test]
    fn test_nested_scopes_reuse_names() {
        let result = parse_and_mangle_top(
            r"
function f1() { let x = 1; return x; }
function f2() { let y = 2; return y; }
",
        );
        // Both inner variables should get 'a' since they're in separate scopes
        // f1 and f2 get their own short names at top level
        assert!(!result.contains("let x"));
        assert!(!result.contains("let y"));
    }

    #[test]
    fn test_var_hoisting() {
        let result = parse_and_mangle_top(
            r"
function test() {
    if (true) {
        var hoisted = 1;
    }
    return hoisted;
}
",
        );
        // var should be hoisted to function scope, both references renamed consistently
        assert!(!result.contains("hoisted"));
    }

    #[test]
    fn test_let_block_scoping() {
        let result = parse_and_mangle_top(
            r"
function test() {
    let outer = 1;
    {
        let inner = 2;
        console.log(inner);
    }
    return outer;
}
",
        );
        assert!(!result.contains("outer"));
        assert!(!result.contains("inner"));
    }

    #[test]
    fn test_global_references_preserved() {
        let result = parse_and_mangle_top("let x = 1; console.log(x); document.title = 'hi';");
        // console and document are globals — never renamed
        assert!(result.contains("console.log"));
        assert!(result.contains("document.title"));
    }

    #[test]
    fn test_property_names_preserved() {
        let result = parse_and_mangle_top("let obj = { myProp: 1 }; let x = obj.myProp;");
        // Property name 'myProp' should not be renamed
        assert!(result.contains("myProp"));
    }

    #[test]
    fn test_shorthand_expansion_object_literal() {
        let result = parse_and_mangle_top("let foo = 1; let obj = { foo };");
        // Should expand shorthand: { foo } → { foo: a }
        assert!(result.contains("foo:"));
        // The variable 'foo' should be renamed
        assert!(!result.contains("let foo"));
    }

    #[test]
    fn test_shorthand_expansion_destructuring() {
        let result = parse_and_mangle_top(
            "let obj = { x: 1 }; let { x } = obj;",
        );
        // Destructuring { x } should expand to { x: a }
        assert!(!result.contains("let { x }"));
    }

    #[test]
    fn test_eval_bailout() {
        let result = parse_and_mangle_top(
            "function test() { let secret = 1; eval('secret'); return secret; }",
        );
        // Should NOT rename because eval is present
        assert!(result.contains("secret"));
    }

    #[test]
    fn test_arrow_function_params() {
        let result = parse_and_mangle_top(
            "let fn = (longName) => longName * 2;",
        );
        assert!(!result.contains("longName"));
    }

    #[test]
    fn test_catch_clause() {
        let result = parse_and_mangle(
            "function f() { try { throw 1; } catch (error) { console.log(error); } }",
            &MangleOptions::default(),
        );
        assert!(!result.contains("error"));
    }

    #[test]
    fn test_for_loop_variable() {
        let result = parse_and_mangle(
            "function f() { for (let index = 0; index < 10; index++) { console.log(index); } }",
            &MangleOptions::default(),
        );
        assert!(!result.contains("index"));
    }

    #[test]
    fn test_reserved_names_not_mangled() {
        let mut reserved = HashSet::new();
        reserved.insert("keepThis".to_string());
        let opts = MangleOptions {
            reserved,
            top_level: true,
        };
        let result = parse_and_mangle("let keepThis = 1; let other = 2;", &opts);
        assert!(result.contains("keepThis"));
        assert!(!result.contains("other"));
    }

    #[test]
    fn test_labels_not_mangled() {
        let result = parse_and_mangle(
            "function f() { outer: for (let i = 0; i < 10; i++) { continue outer; } }",
            &MangleOptions::default(),
        );
        // Label 'outer' should be preserved
        assert!(result.contains("outer:"));
        assert!(result.contains("continue outer"));
    }

    #[test]
    fn test_name_generator_sequence() {
        let mut gen = NameGenerator::new();
        assert_eq!(gen.next(), "a");
        assert_eq!(gen.next(), "b");
        // Skip through to check later names
        for _ in 2..26 {
            gen.next();
        }
        assert_eq!(gen.next(), "A"); // 26
        for _ in 27..52 {
            gen.next();
        }
        assert_eq!(gen.next(), "_"); // 52
        assert_eq!(gen.next(), "$"); // 53
        assert_eq!(gen.next(), "aa"); // 54
        assert_eq!(gen.next(), "ba"); // 55
    }

    #[test]
    fn test_reserved_words_skipped() {
        assert!(is_js_reserved("do"));
        assert!(is_js_reserved("if"));
        assert!(is_js_reserved("in"));
        assert!(is_js_reserved("for"));
        assert!(!is_js_reserved("foo"));
        assert!(!is_js_reserved("a"));
    }

    #[test]
    fn test_multiple_bindings_in_one_declaration() {
        let result = parse_and_mangle_top("let alpha = 1, beta = 2, gamma = 3;");
        assert!(!result.contains("alpha"));
        assert!(!result.contains("beta"));
        assert!(!result.contains("gamma"));
    }

    #[test]
    fn test_class_declaration() {
        let result = parse_and_mangle_top(
            "class MyClass { constructor() { this.x = 1; } } let inst = new MyClass();",
        );
        assert!(!result.contains("MyClass"));
        assert!(!result.contains("inst"));
    }

    #[test]
    fn test_destructuring_array() {
        let result = parse_and_mangle_top("let [first, second] = [1, 2]; console.log(first, second);");
        assert!(!result.contains("first"));
        assert!(!result.contains("second"));
    }

    #[test]
    fn test_top_level_false_preserves_module_names() {
        let result = parse_and_mangle(
            "let moduleVar = 1; function moduleFunc() { let inner = 2; return inner; }",
            &MangleOptions::default(), // top_level: false
        );
        // Module-level names should be preserved
        assert!(result.contains("moduleVar"));
        assert!(result.contains("moduleFunc"));
        // Inner function variable should be mangled
        assert!(!result.contains("inner"));
    }

    #[test]
    fn test_with_statement_bailout() {
        let result = parse_and_mangle_top(
            "function f() { let x = 1; with (obj) { console.log(x); } return x; }",
        );
        // with() should cause bail-out for the entire scope chain
        assert!(result.contains("let x"));
    }

    #[test]
    fn test_function_expression_name() {
        let result = parse_and_mangle_top(
            "let f = function myFunc() { return myFunc; };",
        );
        // myFunc is only visible inside its own scope
        assert!(!result.contains("myFunc"));
    }

    #[test]
    fn test_for_in_loop() {
        let result = parse_and_mangle(
            "function f() { let obj = {}; for (let key in obj) { console.log(key); } }",
            &MangleOptions::default(),
        );
        assert!(!result.contains("key"));
    }

    #[test]
    fn test_for_of_loop() {
        let result = parse_and_mangle(
            "function f() { let arr = []; for (let item of arr) { console.log(item); } }",
            &MangleOptions::default(),
        );
        assert!(!result.contains("item"));
    }

    #[test]
    fn test_computed_key_destructuring() {
        let result = parse_and_mangle_top(
            "let foo = 'x'; let { [foo]: bar } = obj;",
        );
        // Both foo and bar should be mangled, including the computed key reference
        assert!(!result.contains("foo"));
        assert!(!result.contains("bar"));
        // Should produce something like: let a = 'x'; let { [a]: b } = obj;
        assert!(result.contains("[a]"));
    }

    #[test]
    fn test_bundle_wrapper_pattern() {
        // Bundle wrappers use function(mod, exp, req) — params should be mangled
        // even with top_level: false
        let opts = ParserOptions { module: false, ..Default::default() };
        let mut ast = Parser::new(
            r#"var __modules = {};
__modules[1] = function(mod, exp, req) {
    var longVariableName = 42;
    mod.exports = longVariableName;
};"#,
            opts,
        ).parse().unwrap();
        mangle(&mut ast, &MangleOptions::default());
        let result = crate::Codegen::new(&ast, crate::CodegenOptions::default()).generate();
        // Function params should be mangled (they're in function scope, not top-level)
        assert!(!result.contains("longVariableName"));
    }
}
