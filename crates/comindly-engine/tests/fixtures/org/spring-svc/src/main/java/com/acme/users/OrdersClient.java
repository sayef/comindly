package com.acme.users;

import org.springframework.web.client.RestTemplate;

public class OrdersClient {
    private final RestTemplate rest = new RestTemplate();

    public String order(String id) {
        return rest.getForObject("http://orders/api/orders/" + id, String.class);
    }
}
