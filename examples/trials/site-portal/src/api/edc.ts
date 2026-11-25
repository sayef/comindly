import { gateway, type ServiceApi } from "./http";

export interface CaseReportForm {
  id: string;
  formCode: string;
  visitId: string;
  values: Record<string, unknown>;
  status: string;
}

export interface DataQuery {
  id: string;
  formId: string;
  field: string | null;
  text: string;
  status: string;
}

export function subjectForms(subjectId: string, api: ServiceApi = gateway): Promise<CaseReportForm[]> {
  return api.get(`/api/crf/subjects/${subjectId}/forms`);
}

export function saveForm(formId: string, values: Record<string, unknown>, api: ServiceApi = gateway): Promise<CaseReportForm> {
  return api.put(`/api/crf/forms/${formId}`, values);
}

export function openQueries(siteId: string, api: ServiceApi = gateway): Promise<DataQuery[]> {
  return api.get(`/api/queries/sites/${siteId}`);
}

export function answerQuery(queryId: string, answer: string, api: ServiceApi = gateway): Promise<DataQuery> {
  return api.post(`/api/queries/${queryId}/answer`, { answer });
}

/** Data managers lock a form once it is clean; the portal shows the button only to that role. */
export function lockForm(formId: string, api: ServiceApi = gateway): Promise<CaseReportForm> {
  return api.post(`/api/crf/forms/${formId}/lock`, {});
}
