package com.northvile.common;

import org.springframework.web.client.RestTemplate;

/** Writes audit events to audit-trail over HTTP; JVM services use it instead of the topic. */
public class AuditClient {
    private final RestTemplate rest;
    private final String baseUrl;

    public AuditClient(RestTemplate rest, String baseUrl) {
        this.rest = rest;
        this.baseUrl = baseUrl;
    }

    /** Records one change. Fails loudly: an unaudited change to trial data must not commit. */
    public void record(AuditEvent event) {
        BlindingPolicy.requireNoArm(event.after());
        rest.postForObject(baseUrl + "/api/audit/events", event, Void.class);
    }
}
