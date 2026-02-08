//! AST-based import/export extraction using howth-parser.
//!
//! This module provides fast, accurate import extraction using
//! arena-allocated parsing (Bun-style speed optimization).

use howth_parser::fast::{Arena, ArenaParser, ParserOptions};
use howth_parser::fast::{ExportDecl, ImportSpecifier, Stmt, StmtKind};

use crate::bundler::{Import, ImportedName};

/// Extract imports from source code using the arena parser.
///
/// This is much more accurate than regex-based parsing and handles
/// all edge cases (template literals, comments, etc.).
pub fn extract_imports_ast(source: &str) -> Vec<Import> {
    let arena = Arena::new();
    let parser = ArenaParser::new(&arena, source, ParserOptions::default());

    let program = match parser.parse() {
        Ok(p) => p,
        Err(_) => return Vec::new(), // Fall back to empty on parse error
    };

    let mut imports = Vec::new();

    for stmt in program.stmts {
        match stmt.kind {
            StmtKind::Import(import_decl) => {
                let mut names = Vec::new();

                for spec in import_decl.specifiers {
                    match spec {
                        ImportSpecifier::Default { local, .. } => {
                            names.push(ImportedName {
                                imported: "default".to_string(),
                                local: local.to_string(),
                            });
                        }
                        ImportSpecifier::Namespace { local, .. } => {
                            names.push(ImportedName {
                                imported: "*".to_string(),
                                local: local.to_string(),
                            });
                        }
                        ImportSpecifier::Named { imported, local, .. } => {
                            names.push(ImportedName {
                                imported: imported.to_string(),
                                local: local.to_string(),
                            });
                        }
                    }
                }

                imports.push(Import {
                    specifier: import_decl.source.to_string(),
                    dynamic: false,
                    names,
                });
            }
            StmtKind::Export(export_decl) => {
                match export_decl {
                    ExportDecl::All { source, .. } => {
                        // export * from 'module'
                        imports.push(Import {
                            specifier: source.to_string(),
                            dynamic: false,
                            names: vec![ImportedName {
                                imported: "*".to_string(),
                                local: "*".to_string(),
                            }],
                        });
                    }
                    ExportDecl::Named { source: Some(source), specifiers, .. } => {
                        // export { x, y } from 'module'
                        let names = specifiers
                            .iter()
                            .map(|s| ImportedName {
                                imported: s.local.to_string(),
                                local: s.exported.to_string(),
                            })
                            .collect();
                        imports.push(Import {
                            specifier: source.to_string(),
                            dynamic: false,
                            names,
                        });
                    }
                    _ => {
                        // Other exports don't create imports
                    }
                }
            }
            StmtKind::Expr(expr) => {
                // Check for dynamic import() calls
                extract_dynamic_imports_from_expr(&expr, &mut imports);
            }
            StmtKind::Var { decls, .. } => {
                // Check variable initializers for dynamic imports
                for decl in decls.iter() {
                    if let Some(init) = &decl.init {
                        extract_dynamic_imports_from_expr(init, &mut imports);
                    }
                }
            }
            StmtKind::Function(func) => {
                // Check function bodies for dynamic imports
                extract_dynamic_imports_from_stmts(func.body, &mut imports);
            }
            StmtKind::Block(stmts) => {
                extract_dynamic_imports_from_stmts(stmts, &mut imports);
            }
            StmtKind::If { consequent, alternate, .. } => {
                extract_dynamic_imports_from_stmt(consequent, &mut imports);
                if let Some(alt) = alternate {
                    extract_dynamic_imports_from_stmt(alt, &mut imports);
                }
            }
            _ => {}
        }
    }

    imports
}

/// Extract dynamic imports from a slice of statements.
fn extract_dynamic_imports_from_stmts(stmts: &[Stmt<'_>], imports: &mut Vec<Import>) {
    for stmt in stmts {
        extract_dynamic_imports_from_stmt(stmt, imports);
    }
}

/// Extract dynamic imports from a single statement.
fn extract_dynamic_imports_from_stmt(stmt: &Stmt<'_>, imports: &mut Vec<Import>) {
    match &stmt.kind {
        StmtKind::Expr(expr) => {
            extract_dynamic_imports_from_expr(expr, imports);
        }
        StmtKind::Var { decls, .. } => {
            for decl in decls.iter() {
                if let Some(init) = &decl.init {
                    extract_dynamic_imports_from_expr(init, imports);
                }
            }
        }
        StmtKind::Function(func) => {
            extract_dynamic_imports_from_stmts(func.body, imports);
        }
        StmtKind::Block(stmts) => {
            extract_dynamic_imports_from_stmts(stmts, imports);
        }
        StmtKind::If { consequent, alternate, .. } => {
            extract_dynamic_imports_from_stmt(consequent, imports);
            if let Some(alt) = alternate {
                extract_dynamic_imports_from_stmt(alt, imports);
            }
        }
        StmtKind::Return { arg: Some(expr) } => {
            extract_dynamic_imports_from_expr(expr, imports);
        }
        _ => {}
    }
}

/// Recursively extract dynamic imports from expressions.
fn extract_dynamic_imports_from_expr(
    expr: &howth_parser::fast::Expr<'_>,
    imports: &mut Vec<Import>,
) {
    use howth_parser::fast::ExprKind;

    match &expr.kind {
        ExprKind::Import(source_expr) => {
            // import('module')
            if let ExprKind::String(s) = &source_expr.kind {
                imports.push(Import {
                    specifier: s.to_string(),
                    dynamic: true,
                    names: Vec::new(),
                });
            }
        }
        ExprKind::Call { callee, args, .. } => {
            // Check callee and args for nested dynamic imports
            extract_dynamic_imports_from_expr(callee, imports);
            for arg in *args {
                extract_dynamic_imports_from_expr(arg, imports);
            }
        }
        ExprKind::Binary { left, right, .. } => {
            extract_dynamic_imports_from_expr(left, imports);
            extract_dynamic_imports_from_expr(right, imports);
        }
        ExprKind::Conditional { test, consequent, alternate, .. } => {
            extract_dynamic_imports_from_expr(test, imports);
            extract_dynamic_imports_from_expr(consequent, imports);
            extract_dynamic_imports_from_expr(alternate, imports);
        }
        ExprKind::Arrow(arrow) => {
            if let howth_parser::fast::ArrowBody::Expr(body) = &arrow.body {
                extract_dynamic_imports_from_expr(body, imports);
            }
        }
        ExprKind::Await(inner) => {
            extract_dynamic_imports_from_expr(inner, imports);
        }
        ExprKind::Paren(inner) => {
            extract_dynamic_imports_from_expr(inner, imports);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_static_imports() {
        let source = r#"
            import foo from 'foo';
            import { bar, baz as qux } from 'bar';
            import * as utils from './utils';
        "#;

        let imports = extract_imports_ast(source);
        assert_eq!(imports.len(), 3);

        assert_eq!(imports[0].specifier, "foo");
        assert_eq!(imports[0].names.len(), 1);
        assert_eq!(imports[0].names[0].imported, "default");
        assert_eq!(imports[0].names[0].local, "foo");

        assert_eq!(imports[1].specifier, "bar");
        assert_eq!(imports[1].names.len(), 2);

        assert_eq!(imports[2].specifier, "./utils");
        assert_eq!(imports[2].names[0].imported, "*");
    }

    #[test]
    fn test_extract_reexports() {
        let source = r#"
            export { x, y } from 'module';
            export * from 'other';
        "#;

        let imports = extract_imports_ast(source);
        assert_eq!(imports.len(), 2);

        assert_eq!(imports[0].specifier, "module");
        assert_eq!(imports[1].specifier, "other");
    }

    #[test]
    fn test_extract_dynamic_imports() {
        let source = r#"
            const mod = await import('./dynamic');
            const lazy = () => import('lazy-module');
        "#;

        let imports = extract_imports_ast(source);
        assert_eq!(imports.len(), 2);
        assert!(imports[0].dynamic);
        assert!(imports[1].dynamic);
    }

    #[test]
    fn test_handles_parse_errors_gracefully() {
        let source = "this is not valid { javascript";
        let imports = extract_imports_ast(source);
        // Should return empty vec, not panic
        assert!(imports.is_empty());
    }
}
