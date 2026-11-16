function operand_sink(value) { return value; }
function operand_relay(value) { const alias = value; return operand_sink(alias); }

function operand_member(value, other) { return value.perform(other); }

function operand_choice(flag, left, right) { return flag ? left : right; }

function operand_pattern({ ignored }, value) { return value; }

function operand_field(box, value) { box.slot = value; return box.slot; }
function operand_element(box, value) { box[0] = value; return box[1]; }

function operand_rest(...values) { return values[0]; }

function operand_rest_relay(value) { return operand_rest(value); }

function operand_box(value) { const holder = {}; holder.slot = value; const items = [holder.slot]; return items[0]; }

function operand_callback(callback, value) { return callback(value); }

function operand_bound(callback, box, value) { return callback(value); }
class OperandBound { operand_bound_target(value) { return value; } }

function operand_callable_field(box, callback, value) { box.callback = callback; return box.callback(value); }

function operand_apply(callback, value) { return callback.apply(null, [value]); }
function operand_partial_bind(callback, value) { return callback.bind(null, value)(); }

async function operand_deferred_source(value) { return value; }
async function operand_await(value) { return await operand_deferred_source(value); }

function* operand_yield(value, terminal) { yield value; return terminal; }
function operand_generator(value, terminal) { const iterator = operand_yield(value, terminal); const step = iterator.next(); return step.value; }

function* operand_empty_generator(terminal) { return terminal; }
function operand_empty_factory(terminal) { return operand_empty_generator(terminal); }
function* operand_delegated(value, terminal) { yield* [value]; return terminal; }
function operand_delegated_factory(value, terminal) { return operand_delegated(value, terminal); }

function operand_computed(box, key, value) { return box[key](value); }
class OperandConstructed { constructor(value) { this.slot = value; } }
function operand_construct(value) { return new OperandConstructed(value); }
function operand_prototype() { return Object.create(OperandConstructed.prototype); }
function operand_constructor_alias(Factory, value) { const alias = Factory; return new alias(value); }
function operand_constructor_registry(Factory, value) { const registry = [Factory]; return new registry[0](value); }
function operand_not_constructor(Factory) { return Factory.toString(); }
function operand_returning_constructor(value) { return value; }
function operand_construct_return(value) { return new operand_returning_constructor(value); }
