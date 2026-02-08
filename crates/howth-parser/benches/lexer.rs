//! Lexer benchmarks.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use howth_parser::Lexer;

const SAMPLE_SOURCE: &str = r#"
// Sample JavaScript code for benchmarking
function fibonacci(n) {
    if (n <= 1) return n;
    return fibonacci(n - 1) + fibonacci(n - 2);
}

class Calculator {
    constructor() {
        this.result = 0;
    }

    add(x, y) {
        return x + y;
    }

    multiply(x, y) {
        return x * y;
    }

    async fetchData(url) {
        const response = await fetch(url);
        return response.json();
    }
}

const calc = new Calculator();
const numbers = [1, 2, 3, 4, 5].map(n => n * 2);
const { a, b, ...rest } = { a: 1, b: 2, c: 3, d: 4 };
const template = `Hello ${name}, you have ${count} messages`;

export { Calculator, fibonacci };
export default calc;
"#;

fn bench_lexer(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");
    group.throughput(Throughput::Bytes(SAMPLE_SOURCE.len() as u64));

    group.bench_function("sample", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(black_box(SAMPLE_SOURCE));
            loop {
                let token = lexer.next_token();
                if matches!(token.kind, howth_parser::TokenKind::Eof) {
                    break;
                }
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_lexer);
criterion_main!(benches);
