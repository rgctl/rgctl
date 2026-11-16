int operand_sink(int value) { return value; }
int operand_relay(int value) { int alias = value; return operand_sink(alias); }

int operand_choice(int flag, int left, int right) { return flag ? left : right; }

int operand_field(struct OperandBox box, int value) { box.slot = value; return box.slot; }
int operand_element(int box[2], int value) { box[0] = value; return box[1]; }

int operand_callback(int (*callback)(int), int value) { return callback(value); }

typedef int (*OperandLookupCallback)(int);
int operand_computed(OperandLookupCallback* box, int key, int value) { return box[key](value); }
