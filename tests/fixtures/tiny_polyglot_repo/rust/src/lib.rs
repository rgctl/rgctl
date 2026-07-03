pub fn process() {
    helper();
}

pub fn process_labeled(order_id: &str) {
    validate(order_id);
}

pub fn helper() {
    audit();
}

fn validate(order_id: &str) {
    let _ = order_id;
    audit();
}

fn audit() {}
