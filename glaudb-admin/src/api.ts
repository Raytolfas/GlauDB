const API_HOST = window.location.port === "5173" ? "http://localhost:8080" : "";

export function apiUrl(path: string): string {
  return `${API_HOST}${path}`;
}

export function authorizedFetch(token: string, path: string, init: RequestInit = {}) {
  const headers = new Headers(init.headers);
  headers.set("Authorization", `Bearer ${token}`);
  return fetch(apiUrl(path), { ...init, headers });
}
