package com.acme.users;

import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/accounts")
public class UserController {
    private static final String BY_ID = "/{id}";

    @GetMapping(BY_ID)
    public User getUser(@PathVariable String id) {
        return new User();
    }

    @PostMapping
    public User createUser(@RequestBody User u) {
        return u;
    }

    @RequestMapping(value = "/{id}/roles", method = RequestMethod.DELETE)
    public void dropRoles(@PathVariable String id) {
    }
}
