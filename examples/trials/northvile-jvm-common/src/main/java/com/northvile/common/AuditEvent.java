package com.northvile.common;

import java.time.Instant;
import java.util.Map;

/** One audited change: who, when (UTC), what, and the reason a person gave. */
public record AuditEvent(
        String studyId,
        String actor,
        String action,
        String entityType,
        String entityId,
        String reason,
        Map<String, Object> before,
        Map<String, Object> after,
        Instant occurredAt) {

    public static AuditEvent of(StudyId study, String actor, String action, String entityType, String entityId, String reason) {
        return new AuditEvent(study.value(), actor, action, entityType, entityId, reason, Map.of(), Map.of(), UtcClock.now());
    }
}
