func operand_sink(_ value: Int) -> Int { return value }
func operand_relay(_ value: Int) -> Int { let alias = value; return operand_sink(alias) }

func operand_member(_ value: Receiver, _ other: Int) -> Int { return value.perform(other) }
