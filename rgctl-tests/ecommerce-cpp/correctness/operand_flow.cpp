int operand_sink(int value) { return value; }
int operand_relay(int value) { int alias = value; return operand_sink(alias); }

int operand_member(Receiver value, int other) { return value.perform(other); }

class OperandSelf { public: int operand_instance(int other) { return this->perform(other); } };


int operand_choice(bool flag, int left, int right) { return flag ? left : right; }

int operand_field(OperandBox box, int value) { box.slot = value; return box.slot; }
int operand_element(int box[2], int value) { box[0] = value; return box[1]; }

int operand_callback(int (*callback)(int), int value) { return callback(value); }

int operand_bound(Callback callback, OperandSelf box, int value) { return callback(value); }
class OperandBound { public: int operand_bound_target(int value) { return value; } };
