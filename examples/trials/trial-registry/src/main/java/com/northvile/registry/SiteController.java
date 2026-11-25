package com.northvile.registry;

import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

@RestController
@RequestMapping("/api/sites")
public class SiteController {
    private final StudyRepository studies;

    public SiteController(StudyRepository studies) {
        this.studies = studies;
    }

    @GetMapping("/{siteId}")
    public Site getSite(@PathVariable String siteId) {
        return studies.findSite(siteId)
                .orElseThrow(() -> new ResponseStatusException(HttpStatus.NOT_FOUND, "unknown site " + siteId));
    }
}
