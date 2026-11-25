package com.northvile.edc;

import java.util.List;
import java.util.Map;
import java.util.UUID;

import com.northvile.common.AuditClient;
import com.northvile.common.AuditEvent;
import com.northvile.common.BlindingPolicy;
import com.northvile.common.StudyId;
import com.northvile.common.UtcClock;

import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestHeader;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

@RestController
@RequestMapping("/api/crf")
public class CrfController {
    private final FormRepository forms;
    private final EditChecks checks;
    private final AuditClient audit;

    public CrfController(FormRepository forms, EditChecks checks, AuditClient audit) {
        this.forms = forms;
        this.checks = checks;
        this.audit = audit;
    }

    @GetMapping("/subjects/{subjectId}/forms")
    public List<CaseReportForm> listForms(@PathVariable String subjectId) {
        return forms.forSubject(subjectId);
    }

    /** Saves CRF values, runs edit checks and raises a query per failure. Locked forms are refused. */
    @PutMapping("/forms/{formId}")
    public CaseReportForm saveForm(@PathVariable String formId, @RequestBody Map<String, Object> values,
            @RequestHeader("x-northvile-actor") String actor) {
        BlindingPolicy.requireNoArm(values);
        CaseReportForm form = forms.find(formId).orElseThrow(() -> new ResponseStatusException(HttpStatus.NOT_FOUND));
        if (form.locked()) {
            throw new ResponseStatusException(HttpStatus.LOCKED, "form " + formId + " is locked");
        }
        CaseReportForm saved = form.withValues(values, actor, UtcClock.now());
        forms.save(saved);
        audit.record(new AuditEvent(saved.studyId(), actor, "crf.saved", "crf", formId, null, form.values(), values, UtcClock.now()));
        for (String failure : checks.run(saved)) {
            forms.saveQuery(new DataQuery(UUID.randomUUID().toString(), formId, null, null, failure, "edit-check", "OPEN", null, UtcClock.now()));
        }
        return saved;
    }

    /** Data manager lock before analysis; open queries block the lock. */
    @PostMapping("/forms/{formId}/lock")
    public CaseReportForm lockForm(@PathVariable String formId, @RequestHeader("x-northvile-actor") String actor) {
        CaseReportForm form = forms.find(formId).orElseThrow(() -> new ResponseStatusException(HttpStatus.NOT_FOUND));
        CaseReportForm locked = form.lock();
        forms.save(locked);
        audit.record(AuditEvent.of(StudyId.parse(form.studyId()), actor, "crf.locked", "crf", formId, null));
        return locked;
    }
}
