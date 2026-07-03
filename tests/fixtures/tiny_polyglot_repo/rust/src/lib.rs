pub fn process() {
    helper();
}

pub fn process_labeled(order_id: &str) {
    validate(order_id);
}

/// Unique symbol for policy/check subprocess tests (single graph node name).
pub fn unique_root() {
    unique_leaf();
}

pub fn unique_leaf() {}

pub fn helper() {
    audit();
}

fn validate(order_id: &str) {
    let _ = order_id;
    audit();
}

fn audit() {}
