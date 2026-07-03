package com.example;

public class OrderService {
    public void process() {
        helper();
    }

    public void process(String orderId) {
        validate(orderId);
    }

    private void helper() {
        audit();
    }

    private void validate(String orderId) {
        audit();
    }

    private void audit() {}
}
