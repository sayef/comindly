package com.northvile.registry;

import java.util.List;

/** A clinical study as registered by Study Build, e.g. NVC-201. */
public record Study(String id, String title, String phase, String status, String sponsorRole, List<String> siteIds) {

    public boolean enrolling() {
        return "ENROLLING".equals(status);
    }
}
