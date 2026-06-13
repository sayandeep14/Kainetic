import { useQuery } from '@tanstack/react-query';
import { metricsApi } from '../api/metrics';
import { runsApi } from '../api/runs';
import { StatCard } from '../components/StatCard';
import { RunStatusBadge } from '../components/RunStatusBadge';
import { Spinner } from '../components/Spinner';
import { ErrorMessage } from '../components/ErrorMessage';
import { extractErrorMessage } from '../api/client';
import { formatDistanceToNow } from 'date-fns';
import { Link } from 'react-router-dom';

function fmt(usd: number) {
  return usd < 0.01 ? `<$0.01` : `$${usd.toFixed(4)}`;
}
function fmtMs(ms: number | null) {
  if (ms === null) return '—';
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`;
}

export function OverviewPage() {
  const metricsQ = useQuery({ queryKey: ['metrics'], queryFn: metricsApi.get });
  const runsQ = useQuery({
    queryKey: ['runs', 'recent'],
    queryFn: () => runsApi.list({ limit: 10 }),
  });

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-xl font-semibold text-gray-900">Overview</h1>

      {/* Stat cards */}
      {metricsQ.isLoading ? (
        <div className="flex justify-center py-8">
          <Spinner />
        </div>
      ) : metricsQ.error ? (
        <ErrorMessage message={extractErrorMessage(metricsQ.error)} />
      ) : metricsQ.data ? (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
          <StatCard title="Total Runs" value={metricsQ.data.total_runs.toLocaleString()} />
          <StatCard
            title="Success Rate"
            value={
              metricsQ.data.total_runs > 0
                ? `${((metricsQ.data.completed_runs / metricsQ.data.total_runs) * 100).toFixed(1)}%`
                : '—'
            }
          />
          <StatCard title="Total Cost" value={fmt(metricsQ.data.total_cost_usd)} />
          <StatCard
            title="Avg Latency"
            value={fmtMs(metricsQ.data.avg_duration_ms)}
          />
        </div>
      ) : null}

      {/* Recent runs */}
      <div className="rounded-lg border border-gray-200 bg-white shadow-sm">
        <div className="border-b border-gray-200 px-5 py-3">
          <h2 className="text-sm font-semibold text-gray-900">Recent Runs</h2>
        </div>
        {runsQ.isLoading ? (
          <div className="flex justify-center py-8">
            <Spinner />
          </div>
        ) : runsQ.error ? (
          <div className="p-5">
            <ErrorMessage message={extractErrorMessage(runsQ.error)} />
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
                <th className="px-5 py-2 font-medium">Agent</th>
                <th className="px-5 py-2 font-medium">Status</th>
                <th className="px-5 py-2 font-medium">Cost</th>
                <th className="px-5 py-2 font-medium">Duration</th>
                <th className="px-5 py-2 font-medium">Started</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100">
              {(runsQ.data ?? []).map((run) => (
                <tr key={run.id} className="hover:bg-gray-50">
                  <td className="px-5 py-2.5">
                    <Link
                      to={`/runs/${run.id}`}
                      className="font-medium text-brand-600 hover:underline"
                    >
                      {run.agent_name}
                    </Link>
                  </td>
                  <td className="px-5 py-2.5">
                    <RunStatusBadge status={run.status} />
                  </td>
                  <td className="px-5 py-2.5 text-gray-700">{fmt(run.total_cost_usd)}</td>
                  <td className="px-5 py-2.5 text-gray-700">{fmtMs(run.duration_ms)}</td>
                  <td className="px-5 py-2.5 text-gray-500">
                    {formatDistanceToNow(new Date(run.started_at), { addSuffix: true })}
                  </td>
                </tr>
              ))}
              {(runsQ.data ?? []).length === 0 && (
                <tr>
                  <td colSpan={5} className="px-5 py-8 text-center text-gray-400">
                    No runs yet.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        )}
        <div className="border-t border-gray-200 px-5 py-2.5 text-right">
          <Link to="/runs" className="text-xs text-brand-600 hover:underline">
            View all runs →
          </Link>
        </div>
      </div>
    </div>
  );
}
