// Basic class with modifiers
class Animal {
    public name: string;
    protected age: number;
    private _id: number;
    readonly species: string;

    constructor(name: string, age: number, species: string) {
        this.name = name;
        this.age = age;
        this._id = Math.random();
        this.species = species;
    }

    get id(): number {
        return this._id;
    }

    speak(): string {
        return this.name + " says hello";
    }
}

// Abstract class
abstract class Shape {
    abstract area(): number;
    abstract perimeter(): number;

    toString(): string {
        return "Shape(area=" + this.area() + ")";
    }
}

// Implementing abstract class
class Circle extends Shape {
    constructor(private radius: number) {
        super();
    }

    area(): number {
        return Math.PI * this.radius ** 2;
    }

    perimeter(): number {
        return 2 * Math.PI * this.radius;
    }
}

// Class implementing interface
interface Serializable {
    serialize(): string;
    deserialize(data: string): void;
}

class AppConfig implements Serializable {
    data: Record<string, unknown> = {};

    serialize(): string {
        return JSON.stringify(this.data);
    }

    deserialize(data: string): void {
        this.data = JSON.parse(data);
    }
}

// Generic class with constraints
class TypedMap<K extends string | number, V> {
    private store = new Map<K, V>();

    set(key: K, value: V): void {
        this.store.set(key, value);
    }

    get(key: K): V | undefined {
        return this.store.get(key);
    }
}

// Static members
class Counter {
    static count: number = 0;

    static increment(): void {
        Counter.count++;
    }
}

// Parameter properties
class Point {
    constructor(
        public readonly x: number,
        public readonly y: number,
        private label?: string
    ) {}

    distanceTo(other: Point): number {
        return Math.sqrt((this.x - other.x) ** 2 + (this.y - other.y) ** 2);
    }
}

// Override
class SpecialAnimal extends Animal {
    override speak(): string {
        return this.name + " says something special";
    }
}
