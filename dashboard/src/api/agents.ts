import { apiClient } from './client';
import type { Agent } from './types';

export const agentsApi = {
  list: async (): Promise<Agent[]> => {
    const { data } = await apiClient.get<Agent[]>('/agents');
    return data;
  },

  get: async (id: string): Promise<Agent> => {
    const { data } = await apiClient.get<Agent>(`/agents/${id}`);
    return data;
  },

  create: async (body: {
    name: string;
    version?: string;
    description?: string;
    config?: Record<string, unknown>;
  }): Promise<Agent> => {
    const { data } = await apiClient.post<Agent>('/agents', body);
    return data;
  },
};
