import { apiClient } from './client';
import type { Metrics } from './types';

export const metricsApi = {
  get: async (): Promise<Metrics> => {
    const { data } = await apiClient.get<Metrics>('/metrics');
    return data;
  },

  /** Cost per agent over time — derived from runs. */
  costByAgent: async (): Promise<{ agent_name: string; total_cost_usd: number }[]> => {
    const runs = await apiClient.get('/runs', { params: { limit: 200 } });
    const agg = new Map<string, number>();
    for (const run of runs.data as { agent_name: string; total_cost_usd: number }[]) {
      agg.set(run.agent_name, (agg.get(run.agent_name) ?? 0) + run.total_cost_usd);
    }
    return Array.from(agg.entries()).map(([agent_name, total_cost_usd]) => ({
      agent_name,
      total_cost_usd,
    }));
  },
};
