// Basic interface
interface User {
    id: number;
    name: string;
    email?: string;
    readonly createdAt: Date;
}

// Interface extending
interface Admin extends User {
    permissions: string[];
    role: "admin" | "superadmin";
}

// Multiple extends
interface SuperAdmin extends Admin, User {
    superPower: boolean;
}

// Interface with methods
interface Repository<T> {
    find(id: string): Promise<T | null>;
    findAll(): Promise<T[]>;
    create(data: Omit<T, "id">): Promise<T>;
    update(id: string, data: Partial<T>): Promise<T>;
    delete(id: string): Promise<void>;
}

// Interface with call signature
interface Formatter {
    (input: string): string;
    locale: string;
}

// Type aliases
type ID = string | number;
type Nullable<T> = T | null;
type DeepPartial<T> = {
    [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K];
};
type Pick2<T, K extends keyof T> = { [P in K]: T[P] };
type Exclude2<T, U> = T extends U ? never : T;
type NonNullable2<T> = T extends null | undefined ? never : T;
type ReturnType2<T> = T extends (...args: any[]) => infer R ? R : never;
type Parameters2<T> = T extends (...args: infer P) => any ? P : never;
