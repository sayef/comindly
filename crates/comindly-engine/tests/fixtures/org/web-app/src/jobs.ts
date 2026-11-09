import { ServiceClient, services } from "./lib/http";

export async function loadJob(id: string) {
  return fetch(`/api/jobs/${id}`);
}

export async function loadCourse(id: string, api: ServiceClient = services.graph) {
  return api.get(`v1/courses/${id}`);
}
