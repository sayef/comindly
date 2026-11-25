package com.northvile.consent

import com.northvile.common.AuditClient
import com.northvile.common.AuditEvent
import com.northvile.common.StudyId
import org.springframework.scheduling.annotation.Scheduled

/**
 * After an amendment that requires re-consent is approved, every subject still on the old
 * version is flagged; sites see the flag and visits continue only under the old consent's terms.
 */
class ReconsentService(
    private val registry: RegistryClient,
    private val enrolment: EnrolmentGateway,
    private val consents: ConsentRepository,
    private val audit: AuditClient,
) {
    @Scheduled(fixedDelayString = "PT15M")
    fun sweep(studyIds: List<String>) {
        studyIds.forEach { flagSubjects(it) }
    }

    fun flagSubjects(studyId: String): Int {
        val pending = registry.amendments(studyId).filter { it.status == "APPROVED" && it.requiresReconsent }
        var flagged = 0
        for (amendment in pending) {
            for (subject in enrolment.subjectsOn(studyId, amendment.fromVersion)) {
                if (consents.reconsentPending(subject.id)) continue
                consents.flagReconsent(subject.id, amendment.toVersion)
                audit.record(
                    AuditEvent.of(StudyId.parse(studyId), "consent-service", "reconsent.required", "subject", subject.id, amendment.id),
                )
                flagged++
            }
        }
        return flagged
    }
}
