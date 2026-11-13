class Base { func quote(_ value: Int) -> Int { return 11 }
func quote(_ value: String) -> Int { return 21 } }
class Child: Base { override func quote(_ value: Int) -> Int { return 31 } }
