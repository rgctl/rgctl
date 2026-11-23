int operand_sink(int value) { return value; }
int operand_relay(int value) { int alias = value; return operand_sink(alias); }
