package com.northvile.common;

/** Who is acting. Only the unblinded roles may ever see a treatment arm. */
public enum Role {
    SITE_INVESTIGATOR,
    SITE_COORDINATOR,
    CLINICAL_MONITOR,
    DATA_MANAGER,
    SAFETY_PHYSICIAN,
    UNBLINDED_STATISTICIAN;

    public boolean unblinded() {
        return this == SAFETY_PHYSICIAN || this == UNBLINDED_STATISTICIAN;
    }
}
