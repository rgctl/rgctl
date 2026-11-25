func operand_sink(_ value: Int) -> Int { return value }
func operand_relay(_ value: Int) -> Int { let alias = value; return operand_sink(alias) }

func operand_member(_ value: Receiver, _ other: Int) -> Int { return value.perform(other) }

class OperandSelf { func operand_instance(_ other: Int) -> Int { return self.perform(other) } }


func operand_choice(_ flag: Bool, _ left: Any, _ right: Any) -> Any { return flag ? left : right }

func operand_field(_ box: OperandBox, _ value: Any) -> Any { box.slot = value; return box.slot }
func operand_element(_ input: [Any], _ value: Any) -> Any { var box = input; box[0] = value; return box[1] }

func operand_rest(_ values: Any...) -> Any { return values[0] }

func operand_rest_relay(_ value: Any) -> Any { return operand_rest(value) }
