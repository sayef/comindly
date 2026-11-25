import { consentStatus, currentForm, recordSignature } from "../api/consent";
import { enrolSubject, openScreening, submitEligibility } from "../api/enrolment";
import { studySites } from "../api/registry";
import { assertBlinded } from "../blinding";

export type Step = "site" | "consent" | "eligibility" | "enrol" | "done";

/** Walks site staff through screening: consent first, then eligibility, then enrolment. */
export class ScreeningWizard {
  step: Step = "site";
  screeningId?: string;
  protocolVersion?: string;

  constructor(private readonly studyId: string, private readonly language: string) {}

  async start(siteId: string, screeningNumber: string): Promise<void> {
    const sites = await studySites(this.studyId);
    if (!sites.some((s) => s.id === siteId && s.status === "ACTIVE")) {
      throw new Error("This site is not activated for the study yet");
    }
    const screening = await openScreening(this.studyId, siteId, screeningNumber);
    this.screeningId = screening.id;
    this.protocolVersion = screening.protocol_version;
    this.step = "consent";
  }

  /** The form shown must belong to the protocol version the screening was opened under. */
  async sign(screeningNumber: string, witnessRole?: string): Promise<void> {
    const form = await currentForm(this.studyId, this.language);
    if (form.protocolVersion !== this.protocolVersion) {
      throw new Error("The protocol was amended; start a new screening");
    }
    await recordSignature(screeningNumber, this.studyId, form, this.language, witnessRole);
    const status = await consentStatus(screeningNumber);
    if (status.formVersion !== form.formVersion) throw new Error("Signature was not recorded");
    this.step = "eligibility";
  }

  async assess(answers: Record<string, unknown>): Promise<string[]> {
    const result = await submitEligibility(this.screeningId!, answers);
    if (result.eligible) this.step = "enrol";
    return result.failed;
  }

  async enrol(subjectId: string, factors: Record<string, string>): Promise<string> {
    const enrolled = await enrolSubject(this.screeningId!, subjectId, factors);
    assertBlinded(enrolled);
    this.step = "done";
    return enrolled.kit_code;
  }
}
