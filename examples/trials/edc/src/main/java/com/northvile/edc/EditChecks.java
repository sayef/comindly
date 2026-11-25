package com.northvile.edc;

import java.time.LocalDate;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/** Programmed edit checks run on save; each failure becomes a query to the site, not a rejection. */
public class EditChecks {
    private final SourceDataClient source;

    public EditChecks(SourceDataClient source) {
        this.source = source;
    }

    public List<String> run(CaseReportForm form) {
        List<String> failures = new ArrayList<>();
        Map<String, Object> v = form.values();
        Map<String, Object> subject = source.subject(form.subjectId());
        if (!Objects.equals(subject.get("study_id"), form.studyId())) {
            failures.add("subject belongs to another study");
        }
        if (v.containsKey("visit_date")) {
            LocalDate visitDate = LocalDate.parse((String) v.get("visit_date"));
            LocalDate randomised = LocalDate.parse((String) subject.get("randomised_on"));
            if (visitDate.isBefore(randomised)) {
                failures.add("visit_date is before randomisation");
            }
        }
        if ("LAB".equals(form.formCode())) {
            boolean anyResult = !source.labResults(form.subjectId()).isEmpty();
            if (Boolean.TRUE.equals(v.get("samples_taken")) && !anyResult) {
                failures.add("samples_taken but no results received from the central lab");
            }
        }
        if ("AE".equals(form.formCode()) && v.containsKey("adverse_event_id")) {
            Map<String, Object> ae = source.adverseEvent((String) v.get("adverse_event_id"));
            if (!Objects.equals(ae.get("term"), v.get("term"))) {
                failures.add("term differs from the safety database");
            }
        }
        return failures;
    }
}
