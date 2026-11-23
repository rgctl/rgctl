int operand_sink(int value) { return value; }
int operand_relay(int value) { var alias = value; return operand_sink(alias); }

int operand_member(Receiver value, int other) { return value.perform(other); }
