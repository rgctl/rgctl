class Base { int quote(int value) { return 11 }
int quote(String value) { return 21 } }
class Child extends Base { int quote(int value) { return 31 } }
