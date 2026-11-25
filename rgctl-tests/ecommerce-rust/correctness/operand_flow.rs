fn operand_sink(value: i32) -> i32 { return value; }
fn operand_relay(value: i32) -> i32 { let alias = value; return operand_sink(alias); }

fn operand_member(value: Receiver, other: i32) -> i32 { return value.perform(other); }

fn operand_choice(flag: bool, left: i32, right: i32) -> i32 { return if flag { left } else { right }; }

fn operand_field(mut box_value: OperandBox, value: Item) -> Item { box_value.slot = value; return box_value.slot; }
fn operand_element(mut box_value: [Item; 2], value: Item) -> Item { box_value[0] = value; return box_value[1]; }
