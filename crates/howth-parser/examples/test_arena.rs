//! Demonstrate the fast arena-based parser API (Bun-style speed).

use howth_parser::fast::{Arena, ArenaParser, ParserOptions};

fn main() {
    let code = r#"
import React, { useState, useEffect } from 'react';
import * as utils from './utils';

// Functions with template literals
function greet(name) {
    return `Hello, ${name}!`;
}

// Arrow functions with destructuring
const processUser = ({ name, age }) => {
    console.log(`${name} is ${age} years old`);
};

// For-of with spread
const items = [1, 2, 3];
const combined = [...items, 4, 5];
for (const item of combined) {
    console.log(item);
}

// For-in
const obj = { a: 1, b: 2 };
for (const key in obj) {
    console.log(key);
}

// Switch statement
function getStatus(code) {
    switch (code) {
        case 200:
            return 'OK';
        case 404:
            return 'Not Found';
        default:
            return 'Unknown';
    }
}

// Try-catch with optional binding
function safeParse(json) {
    try {
        return JSON.parse(json);
    } catch (e) {
        console.error(`Parse error: ${e.message}`);
        return null;
    } finally {
        console.log('Parse attempt complete');
    }
}

// Class with methods
class Counter {
    constructor(initial = 0) {
        this.count = initial;
    }
    increment() { this.count++; }
    decrement() { this.count--; }
}

// Object spread
const defaults = { theme: 'dark', lang: 'en' };
const config = { ...defaults, lang: 'fr' };

// Do-while
let x = 0;
do {
    x++;
} while (x < 5);

const counter = new Counter();
export default Counter;
export { greet, processUser, safeParse };
"#;

    // Create arena - all AST nodes will be allocated here
    let arena = Arena::new();

    // Parse using the fast arena-based parser
    let opts = ParserOptions::default();
    match ArenaParser::new(&arena, code, opts).parse() {
        Ok(program) => {
            println!("Parsed {} statements", program.stmts.len());
            println!("Arena allocated {} bytes", arena.allocated_bytes());

            // Walk the AST
            for (i, stmt) in program.stmts.iter().enumerate() {
                println!(
                    "  Statement {}: {:?}",
                    i + 1,
                    std::mem::discriminant(&stmt.kind)
                );
            }
        }
        Err(e) => println!("Parse error: {:?}", e),
    }

    // When arena goes out of scope, all AST memory is freed at once
    // No individual deallocations needed - this is what makes it fast!
}
