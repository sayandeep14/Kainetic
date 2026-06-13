import { useQuery } from '@tanstack/react-query';
import { runsApi } from '../api/runs';
import { StatCard } from '../components/StatCard';
import { Spinner } from '../components/Spinner';
import { ErrorMessage } from '../components/ErrorMessage';
import { extractErrorMessage } from '../api/client';
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  BarChart,
  Bar,
} from 'recharts';
import { format } from 'date-fns';
import type { Run } from '../api/types';

/** Returns p-N percentile of a sorted array. */
function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, idx)];
}

function computePercentiles(runs: Run[]) {
  const durations = runs
    .filter((r) => r.duration_ms !== null && r.status === 'completed')
    .map((r) => r.duration_ms as number)
    .sort((a, b) => a - b);
  return {
    p50: percentile(durations, 50),
    p95: percentile(durations, 95),
    p99: percentile(durations, 99),
  };
}

export function LatencyPage() {
  const runsQ = useQuery({
    queryKey: ['runs', 'latency'],
    queryFn: () => runsApi.list({ limit: 200 }),
  });

  const { p50, p95, p99 } = runsQ.data ? computePercentiles(runsQ.data) : { p50: 0, p95: 0, p99: 0 };

  // Latency over time (line chart).
  const latencyTimeline = (() => {
    if (!runsQ.data) return [];
    return runsQ.data
      .filter((r) => r.duration_ms !== null && r.status === 'completed')
      .slice(-50)
      .map((r) => ({
        time: format(new Date(r.started_at), 'HH:mm'),
        ms: r.duration_ms as number,
      }));
  })();

  // P50/P95/P99 by agent.
  const byAgent = (() => {
    if (!runsQ.data) return [];
    const agentRuns = new Map<string, number[]>();
    for (const r of runsQ.data) {
      if (r.duration_ms !== null && r.status === 'completed') {
        const arr = agentRuns.get(r.agent_name) ?? [];
        arr.push(r.duration_ms);
        agentRuns.set(r.agent_name, arr);
      }
    }
    return Array.from(agentRuns.entries()).map(([agent, durations]) => {
      const sorted = [...durations].sort((a, b) => a - b);
      return {
        agent,
        p50: percentile(sorted, 50),
        p95: percentile(sorted, 95),
        p99: percentile(sorted, 99),
      };
    });
  })();

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-xl font-semibold text-gray-900">Latency</h1>

      {runsQ.isLoading ? (
        <div className="flex justify-center py-8">
          <Spinner />
        </div>
      ) : runsQ.error ? (
        <ErrorMessage message={extractErrorMessage(runsQ.error)} />
      ) : (
        <>
          <div className="grid grid-cols-3 gap-4">
            <StatCard title="P50 Latency" value={`${Math.round(p50)}ms`} />
            <StatCard title="P95 Latency" value={`${Math.round(p95)}ms`} />
            <StatCard
              title="P99 Latency"
              value={`${Math.round(p99)}ms`}
              trend={p99 > 2000 ? 'down' : 'neutral'}
              sub={p99 > 2000 ? '⚠ above 2 s threshold' : undefined}
            />
          </div>

          <div className="grid gap-6 lg:grid-cols-2">
            {/* Timeline */}
            <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-sm font-semibold text-gray-900">Latency over time</h2>
              {latencyTimeline.length === 0 ? (
                <p className="py-8 text-center text-sm text-gray-400">No completed runs.</p>
              ) : (
                <ResponsiveContainer width="100%" height={220}>
                  <LineChart data={latencyTimeline}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#f0f0f0" />
                    <XAxis dataKey="time" tick={{ fontSize: 11 }} />
                    <YAxis tick={{ fontSize: 11 }} tickFormatter={(v: number) => `${v}ms`} />
                    <Tooltip formatter={(v: number) => [`${v}ms`, 'Duration']} />
                    <Line
                      type="monotone"
                      dataKey="ms"
                      stroke="#3d57f5"
                      strokeWidth={2}
                      dot={false}
                    />
                  </LineChart>
                </ResponsiveContainer>
              )}
            </div>

            {/* By agent */}
            <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
              <h2 className="mb-4 text-sm font-semibold text-gray-900">P50 / P95 / P99 by agent</h2>
              {byAgent.length === 0 ? (
                <p className="py-8 text-center text-sm text-gray-400">No data.</p>
              ) : (
                <ResponsiveContainer width="100%" height={220}>
                  <BarChart data={byAgent}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#f0f0f0" />
                    <XAxis dataKey="agent" tick={{ fontSize: 10 }} />
                    <YAxis tick={{ fontSize: 11 }} tickFormatter={(v: number) => `${v}ms`} />
                    <Tooltip formatter={(v: number) => [`${v}ms`]} />
                    <Bar dataKey="p50" name="P50" fill="#93aeff" radius={[3, 3, 0, 0]} />
                    <Bar dataKey="p95" name="P95" fill="#3d57f5" radius={[3, 3, 0, 0]} />
                    <Bar dataKey="p99" name="P99" fill="#232fd7" radius={[3, 3, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
