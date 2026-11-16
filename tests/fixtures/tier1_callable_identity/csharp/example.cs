class Base { public virtual int Quote(int value) { return 11; } public virtual int Quote(string value) { return 21; } }
class Child : Base { public override int Quote(int value) { return 31; } }
