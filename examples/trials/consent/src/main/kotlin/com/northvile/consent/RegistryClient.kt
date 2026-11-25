package com.northvile.consent

import org.springframework.cloud.openfeign.FeignClient
import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.PathVariable

data class ProtocolSummary(val studyId: String, val version: String)

data class AmendmentSummary(
    val id: String,
    val fromVersion: String,
    val toVersion: String,
    val requiresReconsent: Boolean,
    val status: String,
)

@FeignClient(name = "trial-registry", path = "/api/studies")
interface RegistryClient {
    @GetMapping("/{studyId}/protocol/current")
    fun currentProtocol(@PathVariable("studyId") studyId: String): ProtocolSummary

    @GetMapping("/{studyId}/amendments")
    fun amendments(@PathVariable("studyId") studyId: String): List<AmendmentSummary>
}
