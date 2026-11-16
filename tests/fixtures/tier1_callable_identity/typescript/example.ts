class Base { quote(value: number): number; quote(value: string): number; quote(value: number | string): number { return 11; } }
class Child extends Base { quote(value: number | string): number { return 21; } }
