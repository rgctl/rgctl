function operand_sink(value: number): number { return value; }
function operand_relay(value: number): number { const alias = value; return operand_sink(alias); }

function operand_member(value: Receiver, other: number): number { return value.perform(other); }
