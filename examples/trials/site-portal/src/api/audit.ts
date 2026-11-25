import { gateway, type ServiceApi } from "./http";

export interface AuditEntry {
  actor: string;
  action: string;
  reason?: string;
  occurred_at: string;
}

/** Who changed a record, when and why; monitors open it from any form or subject. */
export function auditHistory(entityType: string, entityId: string, api: ServiceApi = gateway): Promise<AuditEntry[]> {
  return api.get(`/api/audit/entities/${entityType}/${entityId}`);
}
