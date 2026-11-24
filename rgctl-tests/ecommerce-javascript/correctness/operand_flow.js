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
