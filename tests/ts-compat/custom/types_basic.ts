// Basic type annotations
let count: number = 42;
let name: string = "hello";
let active: boolean = true;
let data: null = null;
let value: undefined = undefined;

// Union and intersection
type StringOrNumber = string | number;
type Named = { name: string } & { age: number };

// Tuple types
type Pair = [string, number];
type VarTuple = [string, ...number[]];

// Array types
let arr: number[] = [1, 2, 3];
let arr2: Array<string> = ["a", "b"];

// Literal types
type Direction = "north" | "south" | "east" | "west";
type Digit = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;

// Parenthesized types
type FnOrNull = (() => void) | null;
type Complex = (string | number) & (string | boolean);

// Function types
type Callback = (err: Error | null, data: string) => void;
type AsyncFn = () => Promise<void>;
type GenericFn = <T>(x: T) => T;

// Object types
type Config = {
    host: string;
    port: number;
    debug?: boolean;
    readonly version: string;
};

// Index signatures
type StringMap = {
    [key: string]: number;
};

// Void and never
function log(msg: string): void {
    console.log(msg);
}

function fail(msg: string): never {
    throw new Error(msg);
}
