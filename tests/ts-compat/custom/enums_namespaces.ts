// Numeric enum
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

// String enum
enum Color {
    Red = "RED",
    Green = "GREEN",
    Blue = "BLUE",
}

// Const enum
const enum HttpStatus {
    OK = 200,
    NotFound = 404,
    InternalError = 500,
}

// Mixed enum
enum Mixed {
    A = 0,
    B = "hello",
    C = 1,
}

// Computed enum
enum FileAccess {
    None,
    Read = 1 << 1,
    Write = 1 << 2,
    ReadWrite = Read | Write,
}

// Namespace
namespace Validation {
    export interface StringValidator {
        isValid(s: string): boolean;
    }

    export class RegExpValidator implements StringValidator {
        constructor(private regexp: RegExp) {}
        isValid(s: string): boolean {
            return this.regexp.test(s);
        }
    }

    export const emailValidator = new RegExpValidator(/^[^@]+@[^@]+$/);
}

// Nested namespace
namespace Outer {
    export namespace Inner {
        export function greet(): string {
            return "hello";
        }
    }
}

const greeting: string = Outer.Inner.greet();
