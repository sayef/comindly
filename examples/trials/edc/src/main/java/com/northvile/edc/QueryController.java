package com.northvile.edc;

import java.util.List;
import java.util.Map;

import com.northvile.common.AuditClient;
import com.northvile.common.AuditEvent;
import com.northvile.common.StudyId;

import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestHeader;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

@RestController
@RequestMapping("/api/queries")
public class QueryController {
    private final FormRepository forms;
    private final AuditClient audit;

    public QueryController(FormRepository forms, AuditClient audit) {
        this.forms = forms;
        this.audit = audit;
    }

    @GetMapping("/sites/{siteId}")
    public List<DataQuery> openQueriesForSite(@PathVariable String siteId) {
        return forms.openQueries(siteId);
    }

    @PostMapping("/{queryId}/answer")
    public DataQuery answerQuery(@PathVariable String queryId, @RequestBody Map<String, String> body,
            @RequestHeader("x-northvile-actor") String actor) {
        DataQuery q = forms.findQuery(queryId).orElseThrow(() -> new ResponseStatusException(HttpStatus.NOT_FOUND));
        DataQuery answered = q.answered(body.get("answer"));
        forms.saveQuery(answered);
        CaseReportForm form = forms.find(q.formId()).orElseThrow();
        audit.record(AuditEvent.of(StudyId.parse(form.studyId()), actor, "query.answered", "query", queryId, body.get("answer")));
        return answered;
    }
}
