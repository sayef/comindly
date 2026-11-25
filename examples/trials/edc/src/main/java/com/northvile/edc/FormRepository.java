package com.northvile.edc;

import java.util.List;
import java.util.Optional;

public interface FormRepository {
    Optional<CaseReportForm> find(String formId);

    List<CaseReportForm> forSubject(String subjectId);

    void save(CaseReportForm form);

    Optional<DataQuery> findQuery(String queryId);

    List<DataQuery> openQueries(String siteId);

    void saveQuery(DataQuery query);
}
