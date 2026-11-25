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

DeferredTask operand_deferred_source(Item value) { co_return value; }
DeferredTask operand_await(Item value) { co_return co_await operand_deferred_source(value); }

Generator operand_yield(Item value, Item terminal) { co_yield value; co_return terminal; }
Generator operand_generator(Item value, Item terminal) { return operand_yield(value, terminal); }

using OperandLookupCallback = int (*)(int);
int operand_computed(OperandLookupCallback* box, int key, int value) { return box[key](value); }
struct OperandConstructed { int slot; explicit OperandConstructed(int value) : slot(value) {} };
OperandConstructed* operand_construct(int value) { return new OperandConstructed(value); }
