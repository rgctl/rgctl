package com.example;

public class OrderController {
    private final OrderService service = new OrderService();

    public void publishEvent() {
        // Leaf handler — unique symbol name for policy/check subprocess tests.
    }

    public void checkout() {
        publishEvent();
        service.process();
        service.process("order-1");
    }
}
