package com.acme.app

import com.acme.lib.Repo
import com.acme.lib.util.Strings

class Service(private val repo: Repo) {
    fun find(id: String): String {
        val item = repo.load(id)
        return Strings.trim(describe(version()))
    }

    fun describe(s: String): String = s + repo.health()

    companion object {
        fun create(): Service = Service(Repo())
    }
}
