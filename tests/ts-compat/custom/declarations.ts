// Declare variable
declare const VERSION: string;
declare let globalConfig: { debug: boolean };

// Declare function
declare function fetchData(url: string): Promise<Response>;
declare function setTimeout(callback: () => void, ms: number): number;

// Declare class
declare class EventEmitter {
    on(event: string, listener: (...args: any[]) => void): this;
    emit(event: string, ...args: any[]): boolean;
    off(event: string, listener: (...args: any[]) => void): this;
}

// Declare namespace
declare namespace NodeJS {
    interface ProcessEnv {
        NODE_ENV: "development" | "production" | "test";
        PORT?: string;
    }
}

// Declare enum
declare enum LogLevel {
    DEBUG,
    INFO,
    WARN,
    ERROR,
}
