package com.northvile.registry;

import java.util.List;
import java.util.Optional;

/** Persistence for studies, protocol versions, amendments and sites. */
public interface StudyRepository {
    Optional<Study> findStudy(String studyId);

    Optional<Protocol> currentProtocol(String studyId);

    Optional<Protocol> protocolVersion(String studyId, String version);

    List<ProtocolAmendment> amendments(String studyId);

    Optional<ProtocolAmendment> amendment(String studyId, String amendmentId);

    void saveAmendment(ProtocolAmendment amendment);

    Optional<Site> findSite(String siteId);

    List<Site> sites(String studyId);
}
