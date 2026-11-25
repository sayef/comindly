package com.northvile.registry;

import java.time.LocalDate;
import java.util.List;
import java.util.Map;

/**
 * One approved protocol version. Consent forms, eligibility and visit windows are all keyed to
 * {@code version}; a subject is always handled under the version they consented to.
 */
public record Protocol(
        String studyId,
        String version,
        LocalDate effectiveFrom,
        List<Map<String, Object>> inclusionCriteria,
        List<Map<String, Object>> exclusionCriteria,
        List<VisitDefinition> schedule,
        RandomisationScheme randomisation,
        Map<String, LabRange> labRanges) {

    /** A planned visit: day relative to randomisation, with an allowed window either side. */
    public record VisitDefinition(String code, String name, int day, int windowBefore, int windowAfter) {
    }

    /** Stratification factors and block size; the allocation list itself lives in randomisation. */
    public record RandomisationScheme(List<String> arms, List<String> strata, int blockSize, Map<String, Integer> ratio) {
    }

    public record LabRange(String unit, double low, double high) {
    }
}
