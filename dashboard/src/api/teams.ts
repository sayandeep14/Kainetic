import { apiClient } from './client';
import type { TeamMember, ApiKey, AuditEntry, CostAlertConfig } from './types';

export const teamsApi = {
  listMembers: async (teamId: string): Promise<TeamMember[]> => {
    const { data } = await apiClient.get<TeamMember[]>(`/teams/${teamId}/members`);
    return data;
  },

  createApiKey: async (teamId: string, name: string): Promise<ApiKey> => {
    const { data } = await apiClient.post<ApiKey>(`/teams/${teamId}/api-keys`, { name });
    return data;
  },

  listAudit: async (): Promise<AuditEntry[]> => {
    const { data } = await apiClient.get<AuditEntry[]>('/audit');
    return data;
  },

  listCostAlerts: async (): Promise<CostAlertConfig[]> => {
    const { data } = await apiClient.get<CostAlertConfig[]>('/alerts');
    return data;
  },

  createCostAlert: async (body: {
    agent_name?: string;
    threshold_usd: number;
    period: string;
    webhook_url?: string;
    notification_email?: string;
  }): Promise<CostAlertConfig> => {
    const { data } = await apiClient.post<CostAlertConfig>('/alerts', body);
    return data;
  },
};

export const authApi = {
  login: async (apiKey: string): Promise<string> => {
    const { data } = await apiClient.post<{ access_token: string }>('/auth/token', {
      api_key: apiKey,
    });
    return data.access_token;
  },
};
