//! Arena-allocated AST node types for JavaScript/TypeScript/JSX.
//!
//! This version uses arena allocation for ~2-3x faster parsing.
//! All nodes are allocated in a bumpalo arena, with no individual heap allocations.

use crate::span::Span;

// =============================================================================
// Expressions
// =============================================================================

/// An expression node.
#[derive(Debug, Clone, Copy)]
pub struct Expr<'a> {
    pub kind: ExprKind<'a>,
    pub span: Span,
}

impl<'a> Expr<'a> {
    pub fn new(kind: ExprKind<'a>, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Expression kinds.
#[derive(Debug, Clone, Copy)]
pub enum ExprKind<'a> {
    // === Literals ===
    Null,
    Bool(bool),
    Number(f64),
    BigInt(&'a str),
    String(&'a str),
    Regex { pattern: &'a str, flags: &'a str },
    TemplateNoSub(&'a str),
    Template {
        quasis: &'a [&'a str],
        exprs: &'a [Expr<'a>],
    },

    // === Identifiers ===
    Ident(&'a str),
    This,
    Super,

    // === Compound Expressions ===
    Array(&'a [Option<Expr<'a>>]),
    Object(&'a [Property<'a>]),
    Function(&'a Function<'a>),
    Arrow(&'a ArrowFunction<'a>),
    Class(&'a Class<'a>),

    // === Operations ===
    Unary {
        op: UnaryOp,
        arg: &'a Expr<'a>,
    },
    Binary {
        op: BinaryOp,
        left: &'a Expr<'a>,
        right: &'a Expr<'a>,
    },
    Assign {
        op: AssignOp,
        left: &'a Expr<'a>,
        right: &'a Expr<'a>,
    },
    Update {
        op: UpdateOp,
        prefix: bool,
        arg: &'a Expr<'a>,
    },
    Conditional {
        test: &'a Expr<'a>,
        consequent: &'a Expr<'a>,
        alternate: &'a Expr<'a>,
    },
    Sequence(&'a [Expr<'a>]),

    // === Member Access ===
    Member {
        object: &'a Expr<'a>,
        property: &'a Expr<'a>,
        computed: bool,
    },
    OptionalMember {
        object: &'a Expr<'a>,
        property: &'a Expr<'a>,
        computed: bool,
    },

    // === Calls ===
    Call {
        callee: &'a Expr<'a>,
        args: &'a [Expr<'a>],
    },
    OptionalCall {
        callee: &'a Expr<'a>,
        args: &'a [Expr<'a>],
    },
    New {
        callee: &'a Expr<'a>,
        args: &'a [Expr<'a>],
    },
    TaggedTemplate {
        tag: &'a Expr<'a>,
        quasi: &'a Expr<'a>,
    },

    // === Special ===
    Spread(&'a Expr<'a>),
    Yield {
        arg: Option<&'a Expr<'a>>,
        delegate: bool,
    },
    Await(&'a Expr<'a>),
    Import(&'a Expr<'a>),
    MetaProperty { meta: &'a str, property: &'a str },

    // Parenthesized (for preserving parens in codegen)
    Paren(&'a Expr<'a>),
}

// =============================================================================
// Statements
// =============================================================================

/// A statement node.
#[derive(Debug, Clone, Copy)]
pub struct Stmt<'a> {
    pub kind: StmtKind<'a>,
    pub span: Span,
}

impl<'a> Stmt<'a> {
    pub fn new(kind: StmtKind<'a>, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Statement kinds.
#[derive(Debug, Clone, Copy)]
pub enum StmtKind<'a> {
    // === Declarations ===
    Var {
        kind: VarKind,
        decls: &'a [VarDeclarator<'a>],
    },
    Function(&'a Function<'a>),
    Class(&'a Class<'a>),

    // === Control Flow ===
    Block(&'a [Stmt<'a>]),
    If {
        test: Expr<'a>,
        consequent: &'a Stmt<'a>,
        alternate: Option<&'a Stmt<'a>>,
    },
    Switch {
        discriminant: Expr<'a>,
        cases: &'a [SwitchCase<'a>],
    },
    For {
        init: Option<ForInit<'a>>,
        test: Option<Expr<'a>>,
        update: Option<Expr<'a>>,
        body: &'a Stmt<'a>,
    },
    ForIn {
        left: ForInit<'a>,
        right: Expr<'a>,
        body: &'a Stmt<'a>,
    },
    ForOf {
        left: ForInit<'a>,
        right: Expr<'a>,
        body: &'a Stmt<'a>,
        is_await: bool,
    },
    While { test: Expr<'a>, body: &'a Stmt<'a> },
    DoWhile { body: &'a Stmt<'a>, test: Expr<'a> },
    Break { label: Option<&'a str> },
    Continue { label: Option<&'a str> },
    Return { arg: Option<Expr<'a>> },
    Throw { arg: Expr<'a> },
    Try {
        block: &'a [Stmt<'a>],
        handler: Option<CatchClause<'a>>,
        finalizer: Option<&'a [Stmt<'a>]>,
    },
    Labeled { label: &'a str, body: &'a Stmt<'a> },

    // === Expressions ===
    Expr(Expr<'a>),
    Empty,
    Debugger,
    With { object: Expr<'a>, body: &'a Stmt<'a> },

    // === Modules ===
    Import(&'a ImportDecl<'a>),
    Export(&'a ExportDecl<'a>),
}

// =============================================================================
// Bindings (Patterns)
// =============================================================================

/// A binding pattern.
#[derive(Debug, Clone, Copy)]
pub struct Binding<'a> {
    pub kind: BindingKind<'a>,
    pub span: Span,
}

impl<'a> Binding<'a> {
    pub fn new(kind: BindingKind<'a>, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Binding pattern kinds.
#[derive(Debug, Clone, Copy)]
pub enum BindingKind<'a> {
    Ident { name: &'a str },
    Array { elements: &'a [Option<ArrayPatternElement<'a>>] },
    Object { properties: &'a [ObjectPatternProperty<'a>] },
}

/// Element in an array pattern.
#[derive(Debug, Clone, Copy)]
pub struct ArrayPatternElement<'a> {
    pub binding: Binding<'a>,
    pub default: Option<Expr<'a>>,
    pub rest: bool,
}

/// Property in an object pattern.
#[derive(Debug, Clone, Copy)]
pub struct ObjectPatternProperty<'a> {
    pub key: PropertyKey<'a>,
    pub value: Binding<'a>,
    pub default: Option<Expr<'a>>,
    pub shorthand: bool,
    pub rest: bool,
}

// =============================================================================
// Supporting Types
// =============================================================================

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Minus, Plus, Not, BitNot, Typeof, Void, Delete,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod, Pow,
    Eq, NotEq, StrictEq, StrictNotEq, Lt, LtEq, Gt, GtEq,
    BitOr, BitXor, BitAnd, Shl, Shr, UShr,
    And, Or, NullishCoalesce,
    In, Instanceof,
}

/// Assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign, AddAssign, SubAssign, MulAssign, DivAssign, ModAssign, PowAssign,
    ShlAssign, ShrAssign, UShrAssign,
    BitOrAssign, BitXorAssign, BitAndAssign,
    AndAssign, OrAssign, NullishAssign,
}

/// Update operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOp {
    Increment, Decrement,
}

/// Variable declaration kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    Var, Let, Const,
}

/// Variable declarator.
#[derive(Debug, Clone, Copy)]
pub struct VarDeclarator<'a> {
    pub binding: Binding<'a>,
    pub init: Option<Expr<'a>>,
    pub span: Span,
}

/// Object property.
#[derive(Debug, Clone, Copy)]
pub struct Property<'a> {
    pub key: PropertyKey<'a>,
    pub value: Expr<'a>,
    pub kind: PropertyKind,
    pub shorthand: bool,
    pub computed: bool,
    pub span: Span,
}

/// Property key.
#[derive(Debug, Clone, Copy)]
pub enum PropertyKey<'a> {
    Ident(&'a str),
    String(&'a str),
    Number(f64),
    Computed(&'a Expr<'a>),
}

/// Property kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    Init, Get, Set, Method,
}

/// Switch case.
#[derive(Debug, Clone, Copy)]
pub struct SwitchCase<'a> {
    pub test: Option<Expr<'a>>,
    pub consequent: &'a [Stmt<'a>],
    pub span: Span,
}

/// Catch clause.
#[derive(Debug, Clone, Copy)]
pub struct CatchClause<'a> {
    pub param: Option<Binding<'a>>,
    pub body: &'a [Stmt<'a>],
    pub span: Span,
}

/// For loop initializer.
#[derive(Debug, Clone, Copy)]
pub enum ForInit<'a> {
    Var { kind: VarKind, decls: &'a [VarDeclarator<'a>] },
    Expr(Expr<'a>),
}

// =============================================================================
// Functions and Classes
// =============================================================================

/// Function node.
#[derive(Debug, Clone, Copy)]
pub struct Function<'a> {
    pub name: Option<&'a str>,
    pub params: &'a [Param<'a>],
    pub body: &'a [Stmt<'a>],
    pub is_async: bool,
    pub is_generator: bool,
    pub span: Span,
}

/// Arrow function node.
#[derive(Debug, Clone, Copy)]
pub struct ArrowFunction<'a> {
    pub params: &'a [Param<'a>],
    pub body: ArrowBody<'a>,
    pub is_async: bool,
    pub span: Span,
}

/// Arrow function body.
#[derive(Debug, Clone, Copy)]
pub enum ArrowBody<'a> {
    Expr(&'a Expr<'a>),
    Block(&'a [Stmt<'a>]),
}

/// Function parameter.
#[derive(Debug, Clone, Copy)]
pub struct Param<'a> {
    pub binding: Binding<'a>,
    pub default: Option<Expr<'a>>,
    pub rest: bool,
    pub span: Span,
}

/// Class node.
#[derive(Debug, Clone, Copy)]
pub struct Class<'a> {
    pub name: Option<&'a str>,
    pub super_class: Option<&'a Expr<'a>>,
    pub body: &'a [ClassMember<'a>],
    pub span: Span,
}

/// Class member.
#[derive(Debug, Clone, Copy)]
pub struct ClassMember<'a> {
    pub kind: ClassMemberKind<'a>,
    pub span: Span,
}

/// Class member kinds.
#[derive(Debug, Clone, Copy)]
pub enum ClassMemberKind<'a> {
    Method {
        key: PropertyKey<'a>,
        value: &'a Function<'a>,
        kind: MethodKind,
        computed: bool,
        is_static: bool,
    },
    Property {
        key: PropertyKey<'a>,
        value: Option<Expr<'a>>,
        computed: bool,
        is_static: bool,
    },
    StaticBlock(&'a [Stmt<'a>]),
    Empty,
}

/// Method kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Method, Get, Set, Constructor,
}

// =============================================================================
// Modules
// =============================================================================

/// Import declaration.
#[derive(Debug, Clone, Copy)]
pub struct ImportDecl<'a> {
    pub specifiers: &'a [ImportSpecifier<'a>],
    pub source: &'a str,
    pub span: Span,
}

/// Import specifier.
#[derive(Debug, Clone, Copy)]
pub enum ImportSpecifier<'a> {
    Default { local: &'a str, span: Span },
    Namespace { local: &'a str, span: Span },
    Named { imported: &'a str, local: &'a str, span: Span },
}

/// Export declaration.
#[derive(Debug, Clone, Copy)]
pub enum ExportDecl<'a> {
    Named {
        specifiers: &'a [ExportSpecifier<'a>],
        source: Option<&'a str>,
        span: Span,
    },
    Default { expr: Expr<'a>, span: Span },
    Decl { decl: &'a Stmt<'a>, span: Span },
    All {
        exported: Option<&'a str>,
        source: &'a str,
        span: Span,
    },
}

/// Export specifier.
#[derive(Debug, Clone, Copy)]
pub struct ExportSpecifier<'a> {
    pub local: &'a str,
    pub exported: &'a str,
    pub span: Span,
}

// =============================================================================
// Program (Root)
// =============================================================================

/// The root AST for a parsed module/script.
#[derive(Debug)]
pub struct Program<'a> {
    pub stmts: &'a [Stmt<'a>],
    pub span: Span,
}

impl<'a> Program<'a> {
    pub fn new(stmts: &'a [Stmt<'a>], span: Span) -> Self {
        Self { stmts, span }
    }
}
