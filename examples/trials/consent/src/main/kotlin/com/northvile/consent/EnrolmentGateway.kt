package com.northvile.consent

import org.springframework.web.reactive.function.client.WebClient

data class EnrolledSubject(val id: String, val siteId: String, val consentVersion: String)

/** Reads enrolled subjects from enrolment to find who must re-consent. */
class EnrolmentGateway(private val client: WebClient) {
    fun subjectsOn(studyId: String, protocolVersion: String): List<EnrolledSubject> =
        client.get()
            .uri("/api/v1/studies/{studyId}/subjects?protocol_version={v}", studyId, protocolVersion)
            .retrieve()
            .bodyToFlux(EnrolledSubject::class.java)
            .collectList()
            .block() ?: emptyList()
}
