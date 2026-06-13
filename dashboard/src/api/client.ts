/** Axios instance wired with JWT auth and base URL. */

import axios from 'axios';

export const BASE_URL = import.meta.env.VITE_API_URL ?? '/v1';

export const apiClient = axios.create({
  baseURL: BASE_URL,
  headers: { 'Content-Type': 'application/json' },
});

/** Injects the stored JWT on every request. */
apiClient.interceptors.request.use((config) => {
  const token = localStorage.getItem('kc_access_token');
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

/** Redirects to /login on 401. */
apiClient.interceptors.response.use(
  (res) => res,
  (err: unknown) => {
    if (axios.isAxiosError(err) && err.response?.status === 401) {
      localStorage.removeItem('kc_access_token');
      window.location.href = '/login';
    }
    return Promise.reject(err);
  },
);

/** Returns a human-readable error message from an Axios error. */
export function extractErrorMessage(err: unknown): string {
  if (axios.isAxiosError(err)) {
    const data = err.response?.data as { error?: string } | undefined;
    return data?.error ?? err.message;
  }
  if (err instanceof Error) return err.message;
  return 'An unexpected error occurred.';
}
