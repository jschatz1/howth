// keyof
interface Person {
    name: string;
    age: number;
    location: string;
}

type PersonKeys = keyof Person;

function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}

// typeof
const config = {
    host: "localhost",
    port: 3000,
    debug: true,
};

type ConfigType = typeof config;

// Indexed access types
type NameType = Person["name"];
type NameOrAge = Person["name" | "age"];

// as expression
const input: unknown = "hello";
const str = input as string;
const num = (input as any) as number;

// satisfies expression
const palette = {
    red: [255, 0, 0],
    green: "#00ff00",
} satisfies Record<string, string | number[]>;

// Non-null assertion
function process(value: string | null) {
    const len = value!.length;
    return len;
}

// Type narrowing with typeof
function padLeft(value: string, padding: string | number) {
    if (typeof padding === "number") {
        return " ".repeat(padding) + value;
    }
    return padding + value;
}

// Type narrowing with instanceof
function printDate(date: Date | string) {
    if (date instanceof Date) {
        console.log(date.toISOString());
    } else {
        console.log(date);
    }
}

// Type predicate
function isString(value: unknown): value is string {
    return typeof value === "string";
}
