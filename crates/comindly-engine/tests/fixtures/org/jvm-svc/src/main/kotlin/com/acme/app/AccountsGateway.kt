package com.acme.app

import org.springframework.web.reactive.function.client.WebClient

class AccountsGateway(private val client: WebClient) {
    fun roles(id: String): String =
        client.delete().uri("/api/accounts/{id}/roles", id).retrieve().toString()
}
