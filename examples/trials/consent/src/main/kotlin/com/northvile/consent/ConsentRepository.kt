package com.northvile.consent

interface ConsentRepository {
    fun currentForm(studyId: String, protocolVersion: String, language: String): ConsentForm?

    fun latestSignature(subjectRef: String): Signature?

    fun save(signature: Signature)

    fun flagReconsent(subjectRef: String, toProtocolVersion: String)

    fun reconsentPending(subjectRef: String): Boolean
}
