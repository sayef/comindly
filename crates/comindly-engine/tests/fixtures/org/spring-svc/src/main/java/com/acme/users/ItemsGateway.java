package com.acme.users;

import org.springframework.web.reactive.function.client.WebClient;

public class ItemsGateway {
    private final WebClient client = WebClient.create("http://items");

    public String item(String id) {
        return client.get().uri("/api/items/{id}", id).retrieve().toString();
    }
}
