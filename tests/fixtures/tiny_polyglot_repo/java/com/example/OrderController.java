package com.example;

public class OrderController {
    private final OrderService service = new OrderService();

    public void checkout() {
        service.process();
        service.process("order-1");
    }
}
