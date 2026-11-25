import { gateway, type ServiceApi } from "./http";

const AE = "/api/v1/adverse-events";

export interface AdverseEventReport {
  subject_id: string;
  term: string;
  onset_date: string;
  grade?: number;
  causality?: "related" | "not_related";
  death?: boolean;
  life_threatening?: boolean;
  hospitalisation?: boolean;
  disability?: boolean;
  congenital_anomaly?: boolean;
  important_medical_event?: boolean;
  narrative?: string;
}

export function reportAdverseEvent(report: AdverseEventReport, api: ServiceApi = gateway): Promise<{ id: string; serious: boolean; susar: boolean }> {
  return api.post(AE, report);
}

export function getAdverseEvent(id: string, api: ServiceApi = gateway) {
  return api.get(`${AE}/${id}`);
}

export function addFollowUp(id: string, text: string, api: ServiceApi = gateway) {
  return api.post(`${AE}/${id}/follow-ups`, { text });
}
