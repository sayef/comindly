package com.northvile.edc;

import java.time.Instant;
import java.util.Map;

/** One CRF instance for a subject's visit. A locked form cannot change without an unlock reason. */
public record CaseReportForm(
        String id,
        String studyId,
        String subjectId,
        String visitId,
        String formCode,
        Map<String, Object> values,
        String status,
        Instant savedAt,
        String savedBy) {

    public boolean locked() {
        return "LOCKED".equals(status);
    }

    public CaseReportForm withValues(Map<String, Object> next, String actor, Instant at) {
        return new CaseReportForm(id, studyId, subjectId, visitId, formCode, next, "SAVED", at, actor);
    }

    public CaseReportForm lock() {
        return new CaseReportForm(id, studyId, subjectId, visitId, formCode, values, "LOCKED", savedAt, savedBy);
    }
}
