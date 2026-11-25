int operand_sink(int value) { return value; }
int operand_relay(int value) { var alias = value; return operand_sink(alias); }

int operand_member(Receiver value, int other) { return value.perform(other); }

class OperandSelf { int operand_instance(int other) { return this.perform(other); } }


Object operand_choice(bool flag, Object left, Object right) { return flag ? left : right; }

Object operand_field(OperandBox box, Object value) { box.slot = value; return box.slot; }
Object operand_element(List<Object> box, Object value) { box[0] = value; return box[1]; }

int operand_callback(int Function(int) callback, int value) { return callback(value); }

int operand_bound(int Function(int) callback, OperandBound box, int value) { return callback(value); }
class OperandBound { int operand_bound_target(int value) { return value; } }
