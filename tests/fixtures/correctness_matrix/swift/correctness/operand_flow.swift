func operand_sink(_ value: Int) -> Int { return value }
func operand_relay(_ value: Int) -> Int { let alias = value; return operand_sink(alias) }

func operand_member(_ value: Receiver, _ other: Int) -> Int { return value.perform(other) }

class OperandSelf { func operand_instance(_ other: Int) -> Int { return self.perform(other) } }


func operand_choice(_ flag: Bool, _ left: Any, _ right: Any) -> Any { return flag ? left : right }

func operand_field(_ box: OperandBox, _ value: Any) -> Any { box.slot = value; return box.slot }
func operand_element(_ input: [Any], _ value: Any) -> Any { var box = input; box[0] = value; return box[1] }

func operand_rest(_ values: Any...) -> Any { return values[0] }

func operand_rest_relay(_ value: Any) -> Any { return operand_rest(value) }

func operand_callback(_ callback: (Int) -> Int, _ value: Int) -> Int { return callback(value) }

func operand_bound(_ callback: (Int) -> Int, _ box: OperandBound, _ value: Int) -> Int { return callback(value) }
class OperandBound { func operand_bound_target(_ value: Int) -> Int { return value } }

func operand_deferred_source(_ value: Item) async -> Item { return value }
func operand_await(_ value: Item) async -> Item { return await operand_deferred_source(value) }

func operand_computed(_ box: [(Int)->Int], _ key: Int, _ value: Int) -> Int { return box[key](value) }
class OperandConstructed { var slot: Int; required init(_ value: Int) { self.slot = value } }
func operand_construct(_ value: Int) -> OperandConstructed { return OperandConstructed(value) }
func operand_constructor_alias(_ factory: OperandConstructed.Type, _ value: Int) -> OperandConstructed { return factory.init(value) }
