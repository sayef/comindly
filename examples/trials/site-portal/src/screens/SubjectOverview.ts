import { addFollowUp, reportAdverseEvent, type AdverseEventReport } from "../api/safety";
import { auditHistory } from "../api/audit";
import { answerQuery, openQueries, saveForm, subjectForms } from "../api/edc";
import { getSubject, withdrawSubject } from "../api/enrolment";
import { kitAssignment } from "../api/randomisation";
import { visitSchedule } from "../api/registry";
import { completeVisit, subjectVisits } from "../api/visits";
import { assertBlinded } from "../blinding";

/** The subject page: kit, visit calendar, forms, open queries and the audit history. */
export async function loadSubjectOverview(subjectId: string) {
  const subject = await getSubject(subjectId);
  assertBlinded(subject as unknown as Record<string, unknown>);
  const [kit, schedule, visits, forms, queries, history] = await Promise.all([
    kitAssignment(subjectId),
    visitSchedule(subject.study_id),
    subjectVisits(subjectId),
    subjectForms(subjectId),
    openQueries(subject.site_id),
    auditHistory("subject", subjectId),
  ]);
  assertBlinded(kit as unknown as Record<string, unknown>);
  return {
    subject,
    kit,
    visits: visits.map((v) => ({ ...v, name: schedule.find((d) => d.code === v.code)?.name ?? v.code })),
    forms,
    queries: queries.filter((q) => forms.some((f) => f.id === q.formId)),
    history,
  };
}

export async function finishVisit(visitId: string, formId: string, values: Record<string, unknown>): Promise<string | null> {
  await saveForm(formId, values);
  const { days_out_of_window } = await completeVisit(visitId);
  return days_out_of_window === 0 ? null : `Visit is ${Math.abs(days_out_of_window)} days outside its window: record a deviation`;
}

/** Serious events must reach safety within 24 hours of the site becoming aware. */
export async function submitAdverseEvent(report: AdverseEventReport): Promise<string> {
  const result = await reportAdverseEvent(report);
  return result.serious ? "Serious adverse event sent to safety" : "Adverse event recorded";
}

export const followUp = addFollowUp;
export const reply = answerQuery;
export const withdraw = withdrawSubject;
