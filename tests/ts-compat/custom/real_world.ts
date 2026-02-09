// Realistic production TypeScript code

// Typed event emitter
interface EventMap {
    connect: { userId: string };
    disconnect: { reason: string };
    message: { channel: string; content: string; timestamp: number };
    error: { code: number; message: string };
}

type EventHandler<T> = (data: T) => void | Promise<void>;

class TypedEventEmitter<Events extends Record<string, any>> {
    private handlers = new Map<keyof Events, Set<EventHandler<any>>>();

    on<K extends keyof Events>(event: K, handler: EventHandler<Events[K]>): this {
        if (!this.handlers.has(event)) {
            this.handlers.set(event, new Set());
        }
        this.handlers.get(event)!.add(handler);
        return this;
    }

    off<K extends keyof Events>(event: K, handler: EventHandler<Events[K]>): this {
        this.handlers.get(event)?.delete(handler);
        return this;
    }

    async emit<K extends keyof Events>(event: K, data: Events[K]): Promise<void> {
        const handlers = this.handlers.get(event);
        if (!handlers) return;
        for (const handler of handlers) {
            await handler(data);
        }
    }
}

// Result type pattern
type Result<T, E = Error> = { ok: true; value: T } | { ok: false; error: E };

function ok<T>(value: T): Result<T, never> {
    return { ok: true, value };
}

function err<E>(error: E): Result<never, E> {
    return { ok: false, error };
}

async function tryFetch<T>(url: string): Promise<Result<T>> {
    try {
        const response = await fetch(url);
        if (!response.ok) {
            return err(new Error("HTTP " + response.status));
        }
        const data = (await response.json()) as T;
        return ok(data);
    } catch (e) {
        return err(e instanceof Error ? e : new Error(String(e)));
    }
}

// Builder pattern with fluent API
class QueryBuilder<T extends Record<string, any>> {
    private conditions: string[] = [];
    private _limit?: number;
    private _offset?: number;

    where<K extends keyof T>(field: K, op: "=" | "!=" | ">" | "<", value: T[K]): this {
        this.conditions.push(String(field) + " " + op + " " + JSON.stringify(value));
        return this;
    }

    limit(n: number): this {
        this._limit = n;
        return this;
    }

    offset(n: number): this {
        this._offset = n;
        return this;
    }

    build(): string {
        let query = "SELECT * FROM table";
        if (this.conditions.length > 0) {
            query += " WHERE " + this.conditions.join(" AND ");
        }
        if (this._limit !== undefined) {
            query += " LIMIT " + this._limit;
        }
        if (this._offset !== undefined) {
            query += " OFFSET " + this._offset;
        }
        return query;
    }
}

// Discriminated union with exhaustive check
type Shape =
    | { kind: "circle"; radius: number }
    | { kind: "rectangle"; width: number; height: number }
    | { kind: "triangle"; base: number; height: number };

function area(shape: Shape): number {
    switch (shape.kind) {
        case "circle":
            return Math.PI * shape.radius ** 2;
        case "rectangle":
            return shape.width * shape.height;
        case "triangle":
            return 0.5 * shape.base * shape.height;
        default: {
            const _exhaustive: never = shape;
            return _exhaustive;
        }
    }
}

// Middleware pattern
type Middleware<Ctx> = (ctx: Ctx, next: () => Promise<void>) => Promise<void>;

class Pipeline<Ctx> {
    private middlewares: Middleware<Ctx>[] = [];

    use(mw: Middleware<Ctx>): this {
        this.middlewares.push(mw);
        return this;
    }

    async execute(ctx: Ctx): Promise<void> {
        const run = (index: number): Promise<void> => {
            if (index >= this.middlewares.length) return Promise.resolve();
            return this.middlewares[index](ctx, () => run(index + 1));
        };
        return run(0);
    }
}

export { TypedEventEmitter, QueryBuilder, Pipeline, ok, err, tryFetch };
export type { EventMap, EventHandler, Result, Middleware, Shape };
