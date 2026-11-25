import { gateway, type ServiceApi } from "./http";

/** The blinded allocation: a kit code to dispense, never the arm. */
export interface KitAssignment {
  subject_id: string;
  kit_code: string;
  randomised_at: string;
}

export function kitAssignment(subjectId: string, api: ServiceApi = gateway): Promise<KitAssignment> {
  return api.get(`/api/randomisations/${subjectId}`);
}
