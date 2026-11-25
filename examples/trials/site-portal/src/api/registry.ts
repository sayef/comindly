import { gateway, type ServiceApi } from "./http";

export interface Site {
  id: string;
  number: string;
  country: string;
  status: string;
}

export interface Study {
  id: string;
  title: string;
  phase: string;
  status: string;
}

export interface VisitDefinition {
  code: string;
  name: string;
  day: number;
  windowBefore: number;
  windowAfter: number;
}

export function getStudy(studyId: string, api: ServiceApi = gateway): Promise<Study> {
  return api.get(`/api/studies/${studyId}`);
}

export function studySites(studyId: string, api: ServiceApi = gateway): Promise<Site[]> {
  return api.get(`/api/studies/${studyId}/sites`);
}

export function visitSchedule(studyId: string, api: ServiceApi = gateway): Promise<VisitDefinition[]> {
  return api.get(`/api/studies/${studyId}/protocol/schedule`);
}
