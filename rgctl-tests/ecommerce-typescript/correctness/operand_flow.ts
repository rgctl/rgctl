function operand_sink(value: number): number { return value; }
function operand_relay(value: number): number { const alias = value; return operand_sink(alias); }

function operand_member(value: Receiver, other: number): number { return value.perform(other); }

function operand_choice(flag: boolean, left: object, right: object): object { return flag ? left : right; }

function operand_pattern({ ignored }: { ignored: number }, value: object): object { return value; }

function operand_field(box: OperandBox, value: object): object { box.slot = value; return box.slot; }
function operand_element(box: object[], value: object): object { box[0] = value; return box[1]; }

function operand_rest(...values: object[]): object { return values[0]; }

function operand_rest_relay(value: object): object { return operand_rest(value); }

function operand_box(value: object): object { const holder: {slot?: object} = {}; holder.slot = value; const items = [holder.slot]; return items[0]; }

function operand_callback(callback: (value: number) => number, value: number): number { return callback(value); }

function operand_bound(callback: (value: number) => number, box: OperandBound, value: number): number { return callback(value); }
class OperandBound { operand_bound_target(value: number): number { return value; } }

function operand_callable_field(box: {callback: (value: number) => number}, callback: (value: number) => number, value: number): number { box.callback = callback; return box.callback(value); }

function operand_apply(callback: any, value: any) { return callback.apply(null, [value]); }
function operand_partial_bind(callback: any, value: any) { return callback.bind(null, value)(); }
