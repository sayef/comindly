export async function request(url: string, init: RequestInit = {}): Promise<Response> {
  return fetch(url, init);
}

export interface ServiceClient {
  get(path: string): Promise<Response>;
  put(path: string, body: unknown): Promise<Response>;
}

class UpstreamClient implements ServiceClient {
  constructor(private readonly base: string) {}

  get(path: string) {
    return this.send("GET", path);
  }

  put(path: string, body: unknown) {
    return this.send("PUT", path);
  }

  private send(method: string, path: string) {
    return request(`${this.base}/${path}`, { method });
  }
}

export const services = { graph: new UpstreamClient("/api/graph") };
