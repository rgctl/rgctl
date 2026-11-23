fn operand_sink(value: i32) -> i32 { return value; }
fn operand_relay(value: i32) -> i32 { let alias = value; return operand_sink(alias); }

fn operand_member(value: Receiver, other: i32) -> i32 { return value.perform(other); }
