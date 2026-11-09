package com.acme.app

import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.PathVariable
import org.springframework.web.bind.annotation.RequestMapping
import org.springframework.web.bind.annotation.RestController

@RestController
@RequestMapping("/api/orders")
class OrderController(private val users: UsersClient) {
    @GetMapping("/{id}")
    fun order(@PathVariable id: String): String = users.getUser(id)
}
