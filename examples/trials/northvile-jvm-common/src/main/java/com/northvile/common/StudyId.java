package com.northvile.common;

import java.util.regex.Pattern;

/** A study identifier such as NVC-201. */
public record StudyId(String value) {
    private static final Pattern FORMAT = Pattern.compile("^NVC-\\d{3}$");

    public StudyId {
        if (value == null || !FORMAT.matcher(value).matches()) {
            throw new IllegalArgumentException("not a study id: " + value);
        }
    }

    public static StudyId parse(String value) {
        return new StudyId(value);
    }
}
