use howth_parser::{parse, ParserOptions};
fn main() {
    let tests = vec![
        // Round 1 fixes
        ("typed arrow", "const f = (n: number) => n;"),
        ("typed multiarg", "const f = (a: string, b: number) => a;"),
        ("index sig", "interface I { [k: string]: number; }"),
        ("optional prop", "interface I { x?: number; }"),
        // Round 2 fixes
        ("optional param", "function foo(x?: number) {}"),
        (
            "optional param multi",
            "function foo(a: string, b?: number, c?: string) {}",
        ),
        (
            "asserts return",
            "function assert(x: unknown): asserts x {}",
        ),
        (
            "asserts is",
            "function isStr(x: unknown): asserts x is string {}",
        ),
        ("private field", "class Foo { #bar = 1; }"),
        ("private method", "class Foo { #doStuff() {} }"),
        ("private typed", "class Foo { #bar: number = 1; }"),
        ("nested generic", "type A = Map<string, Array<number>>;"),
        (
            "triple nested",
            "type A = Map<string, Map<string, Array<number>>>;",
        ),
        ("destructure arrow", "const f = ({ a, b }: Props) => a;"),
        ("array destr arrow", "const f = ([a, b]: string[]) => a;"),
        ("numeric separator", "const x = 1_000_000;"),
        ("for of", "for (const x of items) {}"),
        ("delete expr", "delete obj.prop;"),
        (
            "satisfies",
            "const x = {} satisfies Record<string, string>;",
        ),
        ("this param", "function foo(this: Window) {}"),
        // Round 3 fixes
        (
            "type predicate",
            "function isStr(x: unknown): x is string { return typeof x === 'string'; }",
        ),
        (
            "private expr",
            "class Foo { #x = 1; get() { return this.#x; } }",
        ),
        ("non-null", "const len = value!.length;"),
        ("new generic", "const m = new Map<string, number>();"),
        (
            "abstract method",
            "abstract class Foo { abstract bar(): void; }",
        ),
        ("using decl", "using x = getResource();"),
        (
            "import with",
            "import data from './data.json' with { type: 'json' };",
        ),
        (
            "generic constraint",
            "function foo<T extends string | number>(x: T) {}",
        ),
    ];
    let mut pass = 0;
    let mut _fail = 0;
    for (name, code) in &tests {
        let opts = ParserOptions {
            module: true,
            typescript: true,
            jsx: false,
        };
        match parse(code, opts) {
            Ok(_) => {
                println!("  PASS {}", name);
                pass += 1;
            }
            Err(e) => {
                println!("  FAIL {}: {}", name, e);
                _fail += 1;
            }
        }
    }
    println!("\n{}/{} passed", pass, tests.len());
}
