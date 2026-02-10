//! AST node types for JavaScript/TypeScript/JSX.
//!
//! Design principle: Everything is an Expression, Binding, or Statement.
//! This matches the approach used by esbuild and Bun for simplicity.

use crate::span::Span;

/// The root AST for a parsed module/script.
#[derive(Debug, Clone)]
pub struct Ast {
    /// All statements in the program.
    pub stmts: Vec<Stmt>,
    /// Source code (for error messages and codegen).
    pub source: String,
}

impl Ast {
    /// Create a new AST.
    pub fn new(stmts: Vec<Stmt>, source: String) -> Self {
        Self { stmts, source }
    }
}

// =============================================================================
// Expressions
// =============================================================================

/// Expression node index (for arena allocation).
pub type ExprId = u32;

/// An expression node.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Expression kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    // === Literals ===
    /// Null literal
    Null,
    /// Boolean literal
    Bool(bool),
    /// Number literal
    Number(f64),
    /// BigInt literal (stored as string)
    BigInt(String),
    /// String literal
    String(String),
    /// Regular expression
    Regex { pattern: String, flags: String },
    /// Template literal (no substitutions)
    TemplateNoSub(String),
    /// Template literal with substitutions
    Template {
        quasis: Vec<String>,
        exprs: Vec<Box<Expr>>,
    },

    // === Identifiers ===
    /// Identifier reference
    Ident(String),
    /// `this` keyword
    This,
    /// `super` keyword
    Super,

    // === Compound Expressions ===
    /// Array literal: `[a, b, c]`
    Array(Vec<Option<Box<Expr>>>),
    /// Object literal: `{a: 1, b: 2}`
    Object(Vec<Property>),
    /// Function expression: `function() {}`
    Function(Box<Function>),
    /// Arrow function: `() => {}`
    Arrow(Box<ArrowFunction>),
    /// Class expression: `class {}`
    Class(Box<Class>),

    // === Operations ===
    /// Unary operation: `!x`, `-x`, `typeof x`
    Unary { op: UnaryOp, arg: Box<Expr> },
    /// Binary operation: `a + b`, `a && b`
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Assignment: `a = b`, `a += b`
    Assign {
        op: AssignOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Update expression: `++a`, `a++`
    Update {
        op: UpdateOp,
        prefix: bool,
        arg: Box<Expr>,
    },
    /// Conditional: `a ? b : c`
    Conditional {
        test: Box<Expr>,
        consequent: Box<Expr>,
        alternate: Box<Expr>,
    },
    /// Sequence: `a, b, c`
    Sequence(Vec<Expr>),

    // === Member Access ===
    /// Member expression: `a.b`
    Member {
        object: Box<Expr>,
        property: Box<Expr>,
        computed: bool,
    },
    /// Optional member: `a?.b`
    OptionalMember {
        object: Box<Expr>,
        property: Box<Expr>,
        computed: bool,
    },

    // === Calls ===
    /// Function call: `f(a, b)`
    Call { callee: Box<Expr>, args: Vec<Expr> },
    /// Optional call: `f?.(a, b)`
    OptionalCall { callee: Box<Expr>, args: Vec<Expr> },
    /// New expression: `new Foo(a, b)`
    New { callee: Box<Expr>, args: Vec<Expr> },
    /// Tagged template: `` tag`template` ``
    TaggedTemplate { tag: Box<Expr>, quasi: Box<Expr> },

    // === Special ===
    /// Spread element: `...arr`
    Spread(Box<Expr>),
    /// Yield expression: `yield x`
    Yield {
        arg: Option<Box<Expr>>,
        delegate: bool,
    },
    /// Await expression: `await x`
    Await(Box<Expr>),
    /// Import expression: `import(x)`
    Import(Box<Expr>),
    /// Meta property: `new.target`, `import.meta`
    MetaProperty { meta: String, property: String },

    // === JSX (when feature enabled) ===
    #[cfg(feature = "jsx")]
    JsxElement(Box<JsxElement>),
    #[cfg(feature = "jsx")]
    JsxFragment(Box<JsxFragment>),

    // === TypeScript (when feature enabled) ===
    #[cfg(feature = "typescript")]
    TsAs { expr: Box<Expr>, ty: Box<TsType> },
    #[cfg(feature = "typescript")]
    TsSatisfies { expr: Box<Expr>, ty: Box<TsType> },
    #[cfg(feature = "typescript")]
    TsNonNull(Box<Expr>),
    #[cfg(feature = "typescript")]
    TsTypeAssertion { ty: Box<TsType>, expr: Box<Expr> },
}

// =============================================================================
// Statements
// =============================================================================

/// A statement node.
#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Statement kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    // === Declarations ===
    /// Variable declaration: `let x = 1`
    Var {
        kind: VarKind,
        decls: Vec<VarDeclarator>,
    },
    /// Function declaration: `function foo() {}`
    Function(Box<Function>),
    /// Class declaration: `class Foo {}`
    Class(Box<Class>),

    // === Control Flow ===
    /// Block statement: `{ ... }`
    Block(Vec<Stmt>),
    /// If statement: `if (x) { } else { }`
    If {
        test: Expr,
        consequent: Box<Stmt>,
        alternate: Option<Box<Stmt>>,
    },
    /// Switch statement
    Switch {
        discriminant: Expr,
        cases: Vec<SwitchCase>,
    },
    /// For statement: `for (;;) {}`
    For {
        init: Option<ForInit>,
        test: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },
    /// For-in statement: `for (x in obj) {}`
    ForIn {
        left: ForInit,
        right: Expr,
        body: Box<Stmt>,
    },
    /// For-of statement: `for (x of arr) {}`
    ForOf {
        left: ForInit,
        right: Expr,
        body: Box<Stmt>,
        is_await: bool,
    },
    /// While statement
    While { test: Expr, body: Box<Stmt> },
    /// Do-while statement
    DoWhile { body: Box<Stmt>, test: Expr },
    /// Break statement
    Break { label: Option<String> },
    /// Continue statement
    Continue { label: Option<String> },
    /// Return statement
    Return { arg: Option<Expr> },
    /// Throw statement
    Throw { arg: Expr },
    /// Try statement
    Try {
        block: Vec<Stmt>,
        handler: Option<CatchClause>,
        finalizer: Option<Vec<Stmt>>,
    },
    /// Labeled statement
    Labeled { label: String, body: Box<Stmt> },

    // === Expressions ===
    /// Expression statement
    Expr(Expr),
    /// Empty statement: `;`
    Empty,
    /// Debugger statement
    Debugger,
    /// With statement (deprecated)
    With { object: Expr, body: Box<Stmt> },

    // === Modules ===
    /// Import declaration
    Import(Box<ImportDecl>),
    /// Export declaration
    Export(Box<ExportDecl>),

    // === TypeScript (when feature enabled) ===
    #[cfg(feature = "typescript")]
    TsTypeAlias(Box<TsTypeAlias>),
    #[cfg(feature = "typescript")]
    TsInterface(Box<TsInterface>),
    #[cfg(feature = "typescript")]
    TsEnum(Box<TsEnum>),
    #[cfg(feature = "typescript")]
    TsNamespace(Box<TsNamespace>),
    #[cfg(feature = "typescript")]
    TsDeclare(Box<Stmt>),
}

// =============================================================================
// Bindings (Patterns)
// =============================================================================

/// A binding pattern (used in variable declarations, parameters, etc.)
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub kind: BindingKind,
    pub span: Span,
}

impl Binding {
    pub fn new(kind: BindingKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Binding pattern kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum BindingKind {
    /// Simple identifier: `x`
    Ident {
        name: String,
        #[cfg(feature = "typescript")]
        type_ann: Option<Box<TsType>>,
    },
    /// Array pattern: `[a, b, ...rest]`
    Array {
        elements: Vec<Option<ArrayPatternElement>>,
        #[cfg(feature = "typescript")]
        type_ann: Option<Box<TsType>>,
    },
    /// Object pattern: `{a, b: c, ...rest}`
    Object {
        properties: Vec<ObjectPatternProperty>,
        #[cfg(feature = "typescript")]
        type_ann: Option<Box<TsType>>,
    },
}

/// Element in an array pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayPatternElement {
    pub binding: Binding,
    pub default: Option<Expr>,
    pub rest: bool,
}

/// Property in an object pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectPatternProperty {
    pub key: PropertyKey,
    pub value: Binding,
    pub default: Option<Expr>,
    pub shorthand: bool,
    pub rest: bool,
}

// =============================================================================
// Supporting Types
// =============================================================================

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Minus,  // -
    Plus,   // +
    Not,    // !
    BitNot, // ~
    Typeof, // typeof
    Void,   // void
    Delete, // delete
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Arithmetic
    Add, // +
    Sub, // -
    Mul, // *
    Div, // /
    Mod, // %
    Pow, // **

    // Comparison
    Eq,          // ==
    NotEq,       // !=
    StrictEq,    // ===
    StrictNotEq, // !==
    Lt,          // <
    LtEq,        // <=
    Gt,          // >
    GtEq,        // >=

    // Bitwise
    BitOr,  // |
    BitXor, // ^
    BitAnd, // &
    Shl,    // <<
    Shr,    // >>
    UShr,   // >>>

    // Logical
    And,             // &&
    Or,              // ||
    NullishCoalesce, // ??

    // Other
    In,         // in
    Instanceof, // instanceof
}

/// Assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,        // =
    AddAssign,     // +=
    SubAssign,     // -=
    MulAssign,     // *=
    DivAssign,     // /=
    ModAssign,     // %=
    PowAssign,     // **=
    ShlAssign,     // <<=
    ShrAssign,     // >>=
    UShrAssign,    // >>>=
    BitOrAssign,   // |=
    BitXorAssign,  // ^=
    BitAndAssign,  // &=
    AndAssign,     // &&=
    OrAssign,      // ||=
    NullishAssign, // ??=
}

/// Update operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOp {
    Increment, // ++
    Decrement, // --
}

/// Variable declaration kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    Var,
    Let,
    Const,
}

/// Variable declarator.
#[derive(Debug, Clone, PartialEq)]
pub struct VarDeclarator {
    pub binding: Binding,
    pub init: Option<Expr>,
    pub span: Span,
}

/// Object property.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub key: PropertyKey,
    pub value: Expr,
    pub kind: PropertyKind,
    pub shorthand: bool,
    pub computed: bool,
    pub span: Span,
}

/// Property key.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyKey {
    Ident(String),
    String(String),
    Number(f64),
    Computed(Box<Expr>),
}

/// Property kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    Init,
    Get,
    Set,
    Method,
}

/// Switch case.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    pub test: Option<Expr>, // None for default
    pub consequent: Vec<Stmt>,
    pub span: Span,
}

/// Catch clause.
#[derive(Debug, Clone, PartialEq)]
pub struct CatchClause {
    pub param: Option<Binding>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// For loop initializer.
#[derive(Debug, Clone, PartialEq)]
pub enum ForInit {
    Var {
        kind: VarKind,
        decls: Vec<VarDeclarator>,
    },
    Expr(Expr),
}

// =============================================================================
// Functions and Classes
// =============================================================================

/// Function node (used for declarations, expressions, methods).
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: Option<String>,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub is_async: bool,
    pub is_generator: bool,
    pub span: Span,
    #[cfg(feature = "typescript")]
    pub type_params: Option<Vec<TsTypeParam>>,
    #[cfg(feature = "typescript")]
    pub return_type: Option<Box<TsType>>,
}

/// Arrow function node.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrowFunction {
    pub params: Vec<Param>,
    pub body: ArrowBody,
    pub is_async: bool,
    pub span: Span,
    #[cfg(feature = "typescript")]
    pub type_params: Option<Vec<TsTypeParam>>,
    #[cfg(feature = "typescript")]
    pub return_type: Option<Box<TsType>>,
}

/// Arrow function body.
#[derive(Debug, Clone, PartialEq)]
pub enum ArrowBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

/// Function parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub binding: Binding,
    pub default: Option<Expr>,
    pub rest: bool,
    pub span: Span,
}

/// Class node.
#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub name: Option<String>,
    pub super_class: Option<Box<Expr>>,
    pub body: Vec<ClassMember>,
    pub span: Span,
    #[cfg(feature = "typescript")]
    pub type_params: Option<Vec<TsTypeParam>>,
    #[cfg(feature = "typescript")]
    pub implements: Vec<TsType>,
}

/// Class member.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassMember {
    pub kind: ClassMemberKind,
    pub span: Span,
}

/// Class member kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum ClassMemberKind {
    /// Method: `foo() {}`
    Method {
        key: PropertyKey,
        value: Function,
        kind: MethodKind,
        computed: bool,
        is_static: bool,
        #[cfg(feature = "typescript")]
        accessibility: Option<Accessibility>,
        #[cfg(feature = "typescript")]
        is_abstract: bool,
        #[cfg(feature = "typescript")]
        is_override: bool,
    },
    /// Property: `foo = 1`
    Property {
        key: PropertyKey,
        value: Option<Expr>,
        computed: bool,
        is_static: bool,
        #[cfg(feature = "typescript")]
        type_ann: Option<Box<TsType>>,
        #[cfg(feature = "typescript")]
        accessibility: Option<Accessibility>,
        #[cfg(feature = "typescript")]
        is_readonly: bool,
        #[cfg(feature = "typescript")]
        is_abstract: bool,
        #[cfg(feature = "typescript")]
        is_override: bool,
        #[cfg(feature = "typescript")]
        definite: bool,
    },
    /// Static block: `static { ... }`
    StaticBlock(Vec<Stmt>),
    /// Empty (semicolon)
    Empty,
}

/// Method kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Method,
    Get,
    Set,
    Constructor,
}

/// TypeScript accessibility modifier.
#[cfg(feature = "typescript")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accessibility {
    Public,
    Protected,
    Private,
}

// =============================================================================
// Modules
// =============================================================================

/// Import declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub specifiers: Vec<ImportSpecifier>,
    pub source: String,
    pub span: Span,
    #[cfg(feature = "typescript")]
    pub is_type_only: bool,
}

/// Import specifier.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportSpecifier {
    /// Default import: `import foo from "mod"`
    Default { local: String, span: Span },
    /// Namespace import: `import * as foo from "mod"`
    Namespace { local: String, span: Span },
    /// Named import: `import { foo, bar as baz } from "mod"`
    Named {
        imported: String,
        local: String,
        span: Span,
        #[cfg(feature = "typescript")]
        is_type: bool,
    },
}

/// Export declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportDecl {
    /// Named export: `export { foo, bar }`
    Named {
        specifiers: Vec<ExportSpecifier>,
        source: Option<String>,
        span: Span,
        #[cfg(feature = "typescript")]
        is_type_only: bool,
    },
    /// Default export: `export default expr`
    Default { expr: Expr, span: Span },
    /// Declaration export: `export function foo() {}`
    Decl { decl: Stmt, span: Span },
    /// All export: `export * from "mod"`
    All {
        exported: Option<String>,
        source: String,
        span: Span,
    },
}

/// Export specifier.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportSpecifier {
    pub local: String,
    pub exported: String,
    pub span: Span,
    #[cfg(feature = "typescript")]
    pub is_type: bool,
}

// =============================================================================
// JSX (Feature-gated)
// =============================================================================

#[cfg(feature = "jsx")]
#[derive(Debug, Clone, PartialEq)]
pub struct JsxElement {
    pub opening: JsxOpeningElement,
    pub children: Vec<JsxChild>,
    pub closing: Option<JsxClosingElement>,
    pub span: Span,
}

#[cfg(feature = "jsx")]
#[derive(Debug, Clone, PartialEq)]
pub struct JsxFragment {
    pub children: Vec<JsxChild>,
    pub span: Span,
}

#[cfg(feature = "jsx")]
#[derive(Debug, Clone, PartialEq)]
pub struct JsxOpeningElement {
    pub name: JsxElementName,
    pub attributes: Vec<JsxAttribute>,
    pub self_closing: bool,
    pub span: Span,
}

#[cfg(feature = "jsx")]
#[derive(Debug, Clone, PartialEq)]
pub struct JsxClosingElement {
    pub name: JsxElementName,
    pub span: Span,
}

#[cfg(feature = "jsx")]
#[derive(Debug, Clone, PartialEq)]
pub enum JsxElementName {
    Ident(String),
    MemberExpr(Vec<String>),
    NamespacedName { namespace: String, name: String },
}

#[cfg(feature = "jsx")]
#[derive(Debug, Clone, PartialEq)]
pub enum JsxAttribute {
    Attribute {
        name: JsxAttrName,
        value: Option<JsxAttrValue>,
        span: Span,
    },
    SpreadAttribute {
        argument: Expr,
        span: Span,
    },
}

#[cfg(feature = "jsx")]
#[derive(Debug, Clone, PartialEq)]
pub enum JsxAttrName {
    Ident(String),
    NamespacedName { namespace: String, name: String },
}

#[cfg(feature = "jsx")]
#[derive(Debug, Clone, PartialEq)]
pub enum JsxAttrValue {
    String(String),
    Expr(Expr),
    Element(Box<JsxElement>),
    Fragment(Box<JsxFragment>),
}

#[cfg(feature = "jsx")]
#[derive(Debug, Clone, PartialEq)]
pub enum JsxChild {
    Text(String),
    Element(Box<JsxElement>),
    Fragment(Box<JsxFragment>),
    Expr(Expr),
    Spread(Expr),
}

// =============================================================================
// TypeScript Types (Feature-gated)
// =============================================================================

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsType {
    pub kind: TsTypeKind,
    pub span: Span,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub enum TsTypeKind {
    // Primitive types
    Any,
    Unknown,
    Never,
    Void,
    Null,
    Undefined,
    Boolean,
    Number,
    String,
    Symbol,
    BigInt,
    Object,

    // Literal types
    LitBoolean(bool),
    LitNumber(f64),
    LitString(String),

    // Reference types
    Reference {
        name: String,
        type_args: Option<Vec<TsType>>,
    },
    Qualified {
        left: Box<TsType>,
        right: String,
    },

    // Compound types
    Array(Box<TsType>),
    Tuple(Vec<TsType>),
    Union(Vec<TsType>),
    Intersection(Vec<TsType>),
    Parenthesized(Box<TsType>),

    // Object types
    TypeLiteral(Vec<TsTypeMember>),
    Mapped(Box<TsMappedType>),
    Indexed {
        object: Box<TsType>,
        index: Box<TsType>,
    },

    // Function types
    Function(Box<TsFunctionType>),
    Constructor(Box<TsFunctionType>),

    // Conditional types
    Conditional {
        check: Box<TsType>,
        extends: Box<TsType>,
        true_type: Box<TsType>,
        false_type: Box<TsType>,
    },
    Infer {
        param: TsTypeParam,
    },

    // Other
    Keyof(Box<TsType>),
    Typeof(Box<Expr>),
    This,
    TypePredicate {
        param: String,
        ty: Box<TsType>,
        asserts: bool,
    },
    Import {
        qualifier: String,
        type_args: Option<Vec<TsType>>,
    },
    Template {
        quasis: Vec<String>,
        types: Vec<TsType>,
    },
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsTypeParam {
    pub name: String,
    pub constraint: Option<Box<TsType>>,
    pub default: Option<Box<TsType>>,
    pub span: Span,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsTypeMember {
    pub kind: TsTypeMemberKind,
    pub span: Span,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub enum TsTypeMemberKind {
    Property {
        key: PropertyKey,
        optional: bool,
        readonly: bool,
        type_ann: Option<Box<TsType>>,
    },
    Method {
        key: PropertyKey,
        optional: bool,
        params: Vec<TsFnParam>,
        type_params: Option<Vec<TsTypeParam>>,
        return_type: Option<Box<TsType>>,
    },
    Index {
        readonly: bool,
        param: TsFnParam,
        type_ann: Box<TsType>,
    },
    CallSignature {
        params: Vec<TsFnParam>,
        type_params: Option<Vec<TsTypeParam>>,
        return_type: Option<Box<TsType>>,
    },
    ConstructSignature {
        params: Vec<TsFnParam>,
        type_params: Option<Vec<TsTypeParam>>,
        return_type: Option<Box<TsType>>,
    },
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsFunctionType {
    pub params: Vec<TsFnParam>,
    pub type_params: Option<Vec<TsTypeParam>>,
    pub return_type: Box<TsType>,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsFnParam {
    pub name: Option<String>,
    pub ty: TsType,
    pub optional: bool,
    pub rest: bool,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsMappedType {
    pub type_param: TsTypeParam,
    pub name_type: Option<Box<TsType>>,
    pub optional: Option<TsMappedTypeModifier>,
    pub readonly: Option<TsMappedTypeModifier>,
    pub type_ann: Option<Box<TsType>>,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsMappedTypeModifier {
    Plus,
    Minus,
    None,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsTypeAlias {
    pub name: String,
    pub type_params: Option<Vec<TsTypeParam>>,
    pub ty: TsType,
    pub span: Span,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsInterface {
    pub name: String,
    pub type_params: Option<Vec<TsTypeParam>>,
    pub extends: Vec<TsType>,
    pub body: Vec<TsTypeMember>,
    pub span: Span,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsEnum {
    pub name: String,
    pub is_const: bool,
    pub members: Vec<TsEnumMember>,
    pub span: Span,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsEnumMember {
    pub name: String,
    pub init: Option<Expr>,
    pub span: Span,
}

#[cfg(feature = "typescript")]
#[derive(Debug, Clone, PartialEq)]
pub struct TsNamespace {
    pub name: String,
    pub body: Vec<Stmt>,
    pub span: Span,
}
