package com.northvile.registry;

import java.util.List;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestHeader;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api/studies/{studyId}/amendments")
public class AmendmentController {
    private final StudyRepository studies;
    private final AmendmentService amendments;

    public AmendmentController(StudyRepository studies, AmendmentService amendments) {
        this.studies = studies;
        this.amendments = amendments;
    }

    @GetMapping
    public List<ProtocolAmendment> listAmendments(@PathVariable String studyId) {
        return studies.amendments(studyId);
    }

    @PostMapping
    public ProtocolAmendment submitAmendment(@PathVariable String studyId, @RequestBody ProtocolAmendment draft,
            @RequestHeader("x-northvile-actor") String actor) {
        return amendments.submit(studyId, draft, actor);
    }

    /** Approval by the ethics committee makes the amendment's protocol version current. */
    @PostMapping("/{amendmentId}/approve")
    public ProtocolAmendment approveAmendment(@PathVariable String studyId, @PathVariable String amendmentId,
            @RequestHeader("x-northvile-actor") String actor) {
        return amendments.approve(studyId, amendmentId, actor);
    }
}
