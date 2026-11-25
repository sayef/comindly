package com.northvile.registry;

import com.northvile.common.AuditClient;
import com.northvile.common.AuditEvent;
import com.northvile.common.StudyId;
import com.northvile.common.UtcClock;

import io.awspring.cloud.sns.core.SnsTemplate;

/** Submits and approves protocol amendments and tells dependent services about them. */
public class AmendmentService {
    static final String AMENDMENTS_TOPIC = "protocol-amendments";

    private final StudyRepository studies;
    private final AuditClient audit;
    private final SnsTemplate sns;

    public AmendmentService(StudyRepository studies, AuditClient audit, SnsTemplate sns) {
        this.studies = studies;
        this.audit = audit;
        this.sns = sns;
    }

    public ProtocolAmendment submit(String studyId, ProtocolAmendment draft, String actor) {
        ProtocolAmendment pending = new ProtocolAmendment(draft.id(), studyId, draft.fromVersion(), draft.toVersion(),
                draft.summary(), draft.requiresReconsent(), draft.affectsSchedule(), "SUBMITTED", null);
        studies.saveAmendment(pending);
        audit.record(AuditEvent.of(StudyId.parse(studyId), actor, "amendment.submitted", "amendment", pending.id(), draft.summary()));
        return pending;
    }

    /**
     * Approving publishes protocol.amended; consent asks for re-consent and visit-scheduler
     * recomputes windows from it. Subjects stay on their consented version until they re-consent.
     */
    public ProtocolAmendment approve(String studyId, String amendmentId, String actor) {
        ProtocolAmendment a = studies.amendment(studyId, amendmentId)
                .orElseThrow(() -> new IllegalArgumentException("unknown amendment " + amendmentId));
        if (!"SUBMITTED".equals(a.status())) {
            throw new IllegalStateException("amendment " + amendmentId + " is " + a.status());
        }
        ProtocolAmendment approved = new ProtocolAmendment(a.id(), studyId, a.fromVersion(), a.toVersion(), a.summary(),
                a.requiresReconsent(), a.affectsSchedule(), "APPROVED", UtcClock.now());
        studies.saveAmendment(approved);
        audit.record(AuditEvent.of(StudyId.parse(studyId), actor, "amendment.approved", "amendment", a.id(), a.summary()));
        sns.convertAndSend(AMENDMENTS_TOPIC, approved);
        return approved;
    }
}
