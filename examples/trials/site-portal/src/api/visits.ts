import { gateway, type ServiceApi } from "./http";

export interface Visit {
  id: string;
  code: string;
  target_date: string;
  window: [string, string];
  status: string;
}

export function subjectVisits(subjectId: string, api: ServiceApi = gateway): Promise<Visit[]> {
  return api.get(`/api/v1/subjects/${subjectId}/visits`);
}

export function completeVisit(visitId: string, api: ServiceApi = gateway): Promise<{ days_out_of_window: number }> {
  return api.post(`/api/v1/visits/${visitId}/complete`, {});
}
