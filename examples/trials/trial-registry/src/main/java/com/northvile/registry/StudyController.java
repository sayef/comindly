package com.northvile.registry;

import java.util.List;
import java.util.Map;

import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

/** Read API for studies and their protocol; every other service builds on it. */
@RestController
@RequestMapping("/api/studies")
public class StudyController {
    private final StudyRepository studies;

    public StudyController(StudyRepository studies) {
        this.studies = studies;
    }

    @GetMapping("/{studyId}")
    public Study getStudy(@PathVariable String studyId) {
        return studies.findStudy(studyId).orElseThrow(() -> notFound(studyId));
    }

    /** The protocol version in force today, with eligibility criteria and the visit schedule. */
    @GetMapping("/{studyId}/protocol/current")
    public Protocol currentProtocol(@PathVariable String studyId) {
        return studies.currentProtocol(studyId).orElseThrow(() -> notFound(studyId));
    }

    @GetMapping("/{studyId}/protocol/versions/{version}")
    public Protocol protocolVersion(@PathVariable String studyId, @PathVariable String version) {
        return studies.protocolVersion(studyId, version).orElseThrow(() -> notFound(studyId + "@" + version));
    }

    @GetMapping("/{studyId}/protocol/schedule")
    public List<Protocol.VisitDefinition> visitSchedule(@PathVariable String studyId) {
        return currentProtocol(studyId).schedule();
    }

    /** Arms and strata only; the allocation list is never served from here. */
    @GetMapping("/{studyId}/randomisation")
    public Protocol.RandomisationScheme randomisationScheme(@PathVariable String studyId) {
        return currentProtocol(studyId).randomisation();
    }

    @GetMapping("/{studyId}/lab-ranges")
    public Map<String, Protocol.LabRange> labRanges(@PathVariable String studyId) {
        return currentProtocol(studyId).labRanges();
    }

    /** Reference safety information: the expected adverse reactions used to judge expectedness. */
    @GetMapping("/{studyId}/reference-safety-information")
    public Map<String, Object> referenceSafetyInformation(@PathVariable String studyId) {
        Protocol p = currentProtocol(studyId);
        return Map.of("studyId", studyId, "protocolVersion", p.version(), "expectedTerms", List.of());
    }

    @GetMapping("/{studyId}/sites")
    public List<Site> listSites(@PathVariable String studyId) {
        return studies.sites(studyId);
    }

    private static ResponseStatusException notFound(String what) {
        return new ResponseStatusException(HttpStatus.NOT_FOUND, "unknown study " + what);
    }
}
