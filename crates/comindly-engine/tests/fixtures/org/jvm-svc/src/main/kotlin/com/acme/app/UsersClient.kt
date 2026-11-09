package com.acme.app

import org.springframework.cloud.openfeign.FeignClient
import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.PathVariable

@FeignClient(name = "users", path = "/api/accounts")
interface UsersClient {
    @GetMapping("/{id}")
    fun getUser(@PathVariable("id") id: String): String
}
