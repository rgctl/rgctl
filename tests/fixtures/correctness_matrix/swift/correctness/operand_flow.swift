func operand_sink(_ value: Int) -> Int { return value }
func operand_relay(_ value: Int) -> Int { let alias = value; return operand_sink(alias) }

func operand_member(_ value: Receiver, _ other: Int) -> Int { return value.perform(other) }

class OperandSelf { func operand_instance(_ other: Int) -> Int { return self.perform(other) } }


func operand_choice(_ flag: Bool, _ left: Any, _ right: Any) -> Any { return flag ? left : right }
