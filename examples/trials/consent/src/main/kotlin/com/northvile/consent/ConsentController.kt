package com.northvile.consent

import com.northvile.common.AuditClient
import com.northvile.common.AuditEvent
import com.northvile.common.StudyId
import com.northvile.common.UtcClock
import org.springframework.http.HttpStatus
import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.PathVariable
import org.springframework.web.bind.annotation.PostMapping
import org.springframework.web.bind.annotation.RequestBody
import org.springframework.web.bind.annotation.RequestHeader
import org.springframework.web.bind.annotation.RequestMapping
import org.springframework.web.bind.annotation.RequestParam
import org.springframework.web.bind.annotation.RestController
import org.springframework.web.server.ResponseStatusException

data class SignRequest(val studyId: String, val formVersion: String, val language: String, val witnessRole: String?)

@RestController
@RequestMapping("/api/consent")
class ConsentController(
    private val consents: ConsentRepository,
    private val registry: RegistryClient,
    private val audit: AuditClient,
) {
    /** The form sites must use today: the one approved for the protocol version in force. */
    @GetMapping("/studies/{studyId}/forms/current")
    fun currentForm(@PathVariable studyId: String, @RequestParam language: String): ConsentForm {
        val protocol = registry.currentProtocol(studyId)
        return consents.currentForm(studyId, protocol.version, language)
            ?: throw ResponseStatusException(HttpStatus.NOT_FOUND, "no approved form for ${protocol.version} in $language")
    }

    @GetMapping("/subjects/{subjectRef}")
    fun consentStatus(@PathVariable subjectRef: String): ConsentStatus {
        val latest = consents.latestSignature(subjectRef)
        return ConsentStatus(subjectRef, latest?.formVersion, latest?.protocolVersion, consents.reconsentPending(subjectRef))
    }

    /** Records a signature; refused when the form does not match the current protocol version. */
    @PostMapping("/subjects/{subjectRef}/signatures")
    fun sign(
        @PathVariable subjectRef: String,
        @RequestBody request: SignRequest,
        @RequestHeader("x-northvile-actor") actor: String,
    ): Signature {
        val form = currentForm(request.studyId, request.language)
        if (form.formVersion != request.formVersion) {
            throw ResponseStatusException(HttpStatus.CONFLICT, "form ${request.formVersion} is superseded by ${form.formVersion}")
        }
        val signature = Signature(subjectRef, request.studyId, form.formVersion, form.protocolVersion, UtcClock.now(), actor, request.witnessRole)
        consents.save(signature)
        audit.record(AuditEvent.of(StudyId.parse(request.studyId), actor, "consent.signed", "subject", subjectRef, null))
        return signature
    }
}
