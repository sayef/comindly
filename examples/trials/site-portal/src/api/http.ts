/** Every request carries the signed-in user so services can audit who acted. */
let actor = "anonymous";

export function signIn(user: string): void {
  actor = user;
}

export async function request<T>(url: string, init: RequestInit = {}): Promise<T> {
  const res = await fetch(url, {
    ...init,
    headers: { "content-type": "application/json", "x-northvile-actor": actor, ...(init.headers ?? {}) },
  });
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
  return (await res.json()) as T;
}

export interface ServiceApi {
  get<T>(path: string): Promise<T>;
  post<T>(path: string, body: unknown): Promise<T>;
  put<T>(path: string, body: unknown): Promise<T>;
}

class GatewayApi implements ServiceApi {
  get<T>(path: string) {
    return request<T>(path);
  }

  post<T>(path: string, body: unknown) {
    return request<T>(path, { method: "POST", body: JSON.stringify(body) });
  }

  put<T>(path: string, body: unknown) {
    return request<T>(path, { method: "PUT", body: JSON.stringify(body) });
  }
}

export const gateway: ServiceApi = new GatewayApi();
