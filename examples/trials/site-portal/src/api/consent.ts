import { gateway, type ServiceApi } from "./http";

export interface ConsentForm {
  formVersion: string;
  protocolVersion: string;
  documentKey: string;
}

export interface ConsentStatus {
  formVersion: string | null;
  protocolVersion: string | null;
  reconsentRequired: boolean;
}

export function currentForm(studyId: string, language: string, api: ServiceApi = gateway): Promise<ConsentForm> {
  return api.get(`/api/consent/studies/${studyId}/forms/current?language=${language}`);
}

export function consentStatus(subjectRef: string, api: ServiceApi = gateway): Promise<ConsentStatus> {
  return api.get(`/api/consent/subjects/${subjectRef}`);
}

export function recordSignature(subjectRef: string, studyId: string, form: ConsentForm, language: string, witnessRole?: string, api: ServiceApi = gateway) {
  return api.post(`/api/consent/subjects/${subjectRef}/signatures`, {
    studyId,
    formVersion: form.formVersion,
    language,
    witnessRole: witnessRole ?? null,
  });
}
