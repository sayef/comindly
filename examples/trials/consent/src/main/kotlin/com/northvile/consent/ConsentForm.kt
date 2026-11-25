package com.northvile.consent

import java.time.Instant

/** An approved informed-consent form, tied to exactly one protocol version. */
data class ConsentForm(
    val studyId: String,
    val formVersion: String,
    val protocolVersion: String,
    val language: String,
    val documentKey: String,
)

/** A subject's signature on a form; witnessed signatures carry the witness role. */
data class Signature(
    val subjectRef: String,
    val studyId: String,
    val formVersion: String,
    val protocolVersion: String,
    val signedAt: Instant,
    val signedBy: String,
    val witnessRole: String?,
)

data class ConsentStatus(
    val subjectRef: String,
    val formVersion: String?,
    val protocolVersion: String?,
    val reconsentRequired: Boolean,
)
