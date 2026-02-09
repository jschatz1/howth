// Function overloads
function format(value: string): string;
function format(value: number): string;
function format(value: string | number): string {
    return String(value);
}

// Generic function with constraints
function merge<T extends object, U extends object>(a: T, b: U): T & U {
    return { ...a, ...b };
}

// Optional and default parameters
function greet(name: string, greeting: string = "Hello", punctuation?: string): string {
    return greeting + ", " + name + (punctuation || "!");
}

// Rest parameters with types
function sum(...numbers: number[]): number {
    return numbers.reduce((a, b) => a + b, 0);
}

// Destructured parameters with types
function processUser({ name, age }: { name: string; age: number }): string {
    return name + " is " + age;
}

// Arrow functions with types
const double = (x: number): number => x * 2;
const asyncFetch = async (url: string): Promise<Response> => fetch(url);

// Generator function with types
function* range(start: number, end: number): Generator<number, void, unknown> {
    for (let i = start; i < end; i++) {
        yield i;
    }
}

// Async generator
async function* asyncRange(start: number, end: number): AsyncGenerator<number> {
    for (let i = start; i < end; i++) {
        yield i;
    }
}
