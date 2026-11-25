package com.northvile.edc;

import java.time.Instant;

/** A question raised to a site about a CRF value, by an edit check or a monitor. */
public record DataQuery(
        String id,
        String formId,
        String siteId,
        String field,
        String text,
        String raisedBy,
        String status,
        String answer,
        Instant raisedAt) {

    public DataQuery answered(String reply) {
        return new DataQuery(id, formId, siteId, field, text, raisedBy, "ANSWERED", reply, raisedAt);
    }
}
