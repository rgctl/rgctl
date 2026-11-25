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

Future<Object> operand_deferred_source(Object value) async { return value; }
Future<Object> operand_await(Object value) async { return await operand_deferred_source(value); }

Iterable<Object> operand_yield(Object value, Object terminal) sync* { yield value; }
Object operand_generator(Object value, Object terminal) { final iterator = operand_yield(value, terminal).iterator; iterator.moveNext(); return iterator.current; }

Iterable<Object> operand_empty_generator(Object terminal) sync* { return; }
Object operand_empty_factory(Object terminal) { return operand_empty_generator(terminal); }

int operand_computed(List<int Function(int)> box, int key, int value) { return box[key](value); }
class OperandConstructed { late int slot; OperandConstructed(int value) { this.slot = value; } }
OperandConstructed operand_construct(int value) { return OperandConstructed(value); }
