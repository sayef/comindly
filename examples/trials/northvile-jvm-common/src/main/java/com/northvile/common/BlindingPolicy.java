package com.northvile.common;

import java.util.Map;

/** Keeps treatment arms out of anything a blinded role can read. */
public final class BlindingPolicy {
    public static final String ARM_FIELD = "arm";

    private BlindingPolicy() {
    }

    public static void requireNoArm(Map<String, Object> values) {
        if (values != null && values.containsKey(ARM_FIELD)) {
            throw new IllegalStateException("treatment arm must not leave the randomisation service");
        }
    }

    public static boolean mayUnblind(Role role) {
        return role.unblinded();
    }
}
