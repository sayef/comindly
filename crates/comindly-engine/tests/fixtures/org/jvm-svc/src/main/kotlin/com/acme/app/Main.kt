package com.acme.app

fun main() {
    val s = boot()
    s.find("1")
}

fun boot(): Service = Service.create()

fun version(): String = "1"
