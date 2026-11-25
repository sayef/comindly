const FORBIDDEN = ["arm", "treatment", "allocation"];

/** Throws if a payload about to be shown to site staff carries anything that could unblind them. */
export function assertBlinded(payload: Record<string, unknown>): void {
  for (const key of Object.keys(payload)) {
    if (FORBIDDEN.includes(key.toLowerCase())) {
      throw new Error(`refusing to display unblinding field "${key}"`);
    }
  }
}
