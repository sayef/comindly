package com.northvile.edc;

import java.util.List;
import java.util.Map;

import org.springframework.web.client.RestTemplate;

/** Data EDC cross-checks against: the subject, the visit plan and the lab results. */
public class SourceDataClient {
    private static final String ENROLMENT = "http://enrolment/api/v1/subjects/";
    private static final String VISITS = "http://visit-scheduler/api/v1/subjects/";
    private static final String LABS = "http://lab-ingest/api/lab-results/";
    private static final String SAFETY = "http://safety/api/v1/adverse-events/";

    private final RestTemplate rest;

    public SourceDataClient(RestTemplate rest) {
        this.rest = rest;
    }

    @SuppressWarnings("unchecked")
    public Map<String, Object> subject(String subjectId) {
        return rest.getForObject(ENROLMENT + subjectId, Map.class);
    }

    @SuppressWarnings("unchecked")
    public List<Map<String, Object>> visits(String subjectId) {
        return rest.getForObject(VISITS + subjectId + "/visits", List.class);
    }

    @SuppressWarnings("unchecked")
    public List<Map<String, Object>> labResults(String subjectId) {
        return rest.getForObject(LABS + subjectId, List.class);
    }

    @SuppressWarnings("unchecked")
    public Map<String, Object> adverseEvent(String aeId) {
        return rest.getForObject(SAFETY + aeId, Map.class);
    }
}
