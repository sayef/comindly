import { gateway, type ServiceApi } from "./http";

const ENROLMENT = "/api/v1";

export interface Screening {
  id: string;
  protocol_version: string;
}

export interface Subject {
  id: string;
  study_id: string;
  site_id: string;
  status: string;
  kit_code: string;
  randomised_on: string;
}

export function openScreening(studyId: string, siteId: string, screeningNumber: string, api: ServiceApi = gateway): Promise<Screening> {
  return api.post(`${ENROLMENT}/screenings`, { study_id: studyId, site_id: siteId, screening_number: screeningNumber });
}

export function submitEligibility(screeningId: string, answers: Record<string, unknown>, api: ServiceApi = gateway): Promise<{ eligible: boolean; failed: string[] }> {
  return api.post(`${ENROLMENT}/screenings/${screeningId}/eligibility`, answers);
}

export function enrolSubject(screeningId: string, subjectId: string, factors: Record<string, string>, api: ServiceApi = gateway): Promise<{ subject_id: string; kit_code: string }> {
  return api.post(`${ENROLMENT}/screenings/${screeningId}/enrol`, { subject_id: subjectId, factors });
}

export async function getSubject(subjectId: string): Promise<Subject> {
  const res = await fetch(`${ENROLMENT}/subjects/${subjectId}`);
  return (await res.json()) as Subject;
}

export function withdrawSubject(subjectId: string, reason: string, api: ServiceApi = gateway): Promise<Subject> {
  return api.post(`${ENROLMENT}/subjects/${subjectId}/withdraw`, { reason });
}
