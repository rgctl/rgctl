open class Base {
    open fun quote(value: Int): Int {
        return 11
    }
    open fun quote(value: String): Int {
        return 21
    }
}
class Child : Base() {
    override fun quote(value: Int): Int {
        return 31
    }
}
