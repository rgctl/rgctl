class Base { def quote(value: Int): Int = { return 11 }
def quote(value: String): Int = { return 21 } }
class Child extends Base { override def quote(value: Int): Int = { return 31 } }
