use howth_parser::{parse, transform, CodegenOptions, ParserOptions};

fn main() {
    let code = r#"
function greet(name) {
    const message = "Hello, " + name;
    return message;
}

const result = greet("world");
console.log(result);
"#;

    // Parse to AST
    let opts = ParserOptions::default();
    match parse(code, opts) {
        Ok(ast) => {
            println!("Parsed {} statements", ast.stmts.len());
        }
        Err(e) => println!("Parse error: {:?}", e),
    }

    // Transform (parse + codegen)
    let parser_opts = ParserOptions::default();
    let codegen_opts = CodegenOptions::default();
    match transform(code, parser_opts, codegen_opts) {
        Ok(output) => {
            println!("\nRegenerated:\n{}", output);
        }
        Err(e) => println!("Error: {:?}", e),
    }

    // Minified
    let parser_opts = ParserOptions::default();
    let codegen_opts = CodegenOptions {
        minify: true,
        ..Default::default()
    };
    match transform(code, parser_opts, codegen_opts) {
        Ok(output) => {
            println!("\n--- Minified ---\n{}", output);
        }
        Err(e) => println!("Error: {:?}", e),
    }
}
