function operand_sink(value) { return value; }
function operand_relay(value) { const alias = value; return operand_sink(alias); }

function operand_member(value, other) { return value.perform(other); }

function operand_choice(flag, left, right) { return flag ? left : right; }

function operand_pattern({ ignored }, value) { return value; }
