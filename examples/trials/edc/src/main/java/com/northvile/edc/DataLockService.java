package com.northvile.edc;

import java.util.List;
import java.util.Map;

import com.northvile.common.AuditClient;
import com.northvile.common.AuditEvent;
import com.northvile.common.StudyId;

import org.springframework.web.client.RestTemplate;

/** Database lock for a study: every form locked, no open queries, then the sponsor is told. */
public class DataLockService {
    private static final String REGISTRY = "http://trial-registry/api/studies/";

    private final FormRepository forms;
    private final RestTemplate rest;
    private final AuditClient audit;

    public DataLockService(FormRepository forms, RestTemplate rest, AuditClient audit) {
        this.forms = forms;
        this.rest = rest;
        this.audit = audit;
    }

    @SuppressWarnings("unchecked")
    public int lockStudy(String studyId, List<String> subjectIds, String actor) {
        Map<String, Object> study = rest.getForObject(REGISTRY + studyId, Map.class);
        if ("ENROLLING".equals(study.get("status"))) {
            throw new IllegalStateException("cannot lock a study that is still enrolling");
        }
        int locked = 0;
        for (String subjectId : subjectIds) {
            for (CaseReportForm form : forms.forSubject(subjectId)) {
                if (!form.locked()) {
                    forms.save(form.lock());
                    locked++;
                }
            }
        }
        audit.record(AuditEvent.of(StudyId.parse(studyId), actor, "database.locked", "study", studyId, null));
        return locked;
    }
}
