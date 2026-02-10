use howth_parser::{Parser, ParserOptions};
use std::{env, fs};
fn main() {
    let path = env::args().nth(1).expect("need file path");
    let source = fs::read_to_string(&path).expect("cannot read file");
    let source = source.strip_prefix('\u{feff}').unwrap_or(&source);
    let is_tsx = path.ends_with(".tsx");
    let opts = ParserOptions {
        module: true,
        jsx: is_tsx,
        typescript: true,
        ..Default::default()
    };
    match Parser::new(source, opts).parse() {
        Ok(_) => println!("PASS"),
        Err(e) => {
            eprintln!("FAIL: {}", e);
            std::process::exit(1);
        }
    }
}
