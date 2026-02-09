// Generic functions
function identity<T>(x: T): T {
    return x;
}

function pair<A, B>(a: A, b: B): [A, B] {
    return [a, b];
}

// Generic constraints
function getLength<T extends { length: number }>(x: T): number {
    return x.length;
}

// Default type parameters
function createArray<T = string>(length: number, value: T): T[] {
    return Array(length).fill(value);
}

// Generic classes
class Container<T> {
    private value: T;
    constructor(val: T) {
        this.value = val;
    }
    get(): T {
        return this.value;
    }
    map<U>(fn: (v: T) => U): Container<U> {
        return new Container(fn(this.value));
    }
}

// Conditional types
type IsString<T> = T extends string ? true : false;
type Flatten<T> = T extends Array<infer U> ? U : T;
type UnwrapPromise<T> = T extends Promise<infer U> ? UnwrapPromise<U> : T;

// Mapped types
type Readonly2<T> = { readonly [K in keyof T]: T[K] };
type Optional<T> = { [K in keyof T]?: T[K] };
type Mutable<T> = { -readonly [K in keyof T]: T[K] };
type Required2<T> = { [K in keyof T]-?: T[K] };

// Mapped type with as clause
type Getters<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};

// Template literal types
type EventName<T extends string> = `on${Capitalize<T>}`;
