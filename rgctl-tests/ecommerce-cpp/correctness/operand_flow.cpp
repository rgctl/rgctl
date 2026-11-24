int operand_sink(int value) { return value; }
int operand_relay(int value) { int alias = value; return operand_sink(alias); }

int operand_member(Receiver value, int other) { return value.perform(other); }

class OperandSelf { public: int operand_instance(int other) { return this->perform(other); } };


int operand_choice(bool flag, int left, int right) { return flag ? left : right; }
