package com.northvile.registry;

import java.time.Instant;

/** A change to the protocol; once approved it produces a new protocol version. */
public record ProtocolAmendment(
        String id,
        String studyId,
        String fromVersion,
        String toVersion,
        String summary,
        boolean requiresReconsent,
        boolean affectsSchedule,
        String status,
        Instant approvedAt) {
}
