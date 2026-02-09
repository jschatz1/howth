// Type-only imports
import type { User } from "./types";
import type { Config as AppConfig } from "./config";

// Mixed imports with type keyword on specifiers
import { createUser, type UserRole } from "./users";

// Type-only export
export type { User };
export type { AppConfig as Config };

// Re-export types
export type { Repository } from "./repository";

// Export interface
export interface ApiResponse<T> {
    data: T;
    status: number;
    message: string;
}

// Export type alias
export type Maybe<T> = T | null | undefined;
export type AsyncResult<T> = Promise<{ ok: true; value: T } | { ok: false; error: Error }>;

// Export const with type
export const DEFAULT_TIMEOUT: number = 5000;

// Export function with generics
export function mapResult<T, U>(
    result: { ok: boolean; value?: T; error?: Error },
    fn: (value: T) => U
): { ok: boolean; value?: U; error?: Error } {
    if (result.ok && result.value !== undefined) {
        return { ok: true, value: fn(result.value) };
    }
    return result as any;
}

// Export class
export class HttpClient {
    constructor(private baseUrl: string) {}

    async get<T>(path: string): Promise<T> {
        const res = await fetch(this.baseUrl + path);
        return res.json() as Promise<T>;
    }
}
