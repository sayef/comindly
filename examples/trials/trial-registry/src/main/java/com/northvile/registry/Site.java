package com.northvile.registry;

import java.time.LocalDate;

/** A trial site. Screening is only allowed once the site is activated for the study. */
public record Site(String id, String studyId, String number, String country, String status, LocalDate activatedOn, String safetyContactRole) {

    public boolean activated() {
        return "ACTIVE".equals(status);
    }
}
