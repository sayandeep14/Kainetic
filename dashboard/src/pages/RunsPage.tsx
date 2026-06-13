import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { runsApi } from '../api/runs';
import type { RunStatus } from '../api/types';
import { RunStatusBadge } from '../components/RunStatusBadge';
import { Spinner } from '../components/Spinner';
import { ErrorMessage } from '../components/ErrorMessage';
import { extractErrorMessage } from '../api/client';
import { formatDistanceToNow } from 'date-fns';
import { Search, ChevronLeft, ChevronRight } from 'lucide-react';

const PAGE_SIZE = 25;

function fmt(usd: number) {
  return usd < 0.01 ? '<$0.01' : `$${usd.toFixed(4)}`;
}
function fmtMs(ms: number | null) {
  if (ms === null) return '—';
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`;
}

export function RunsPage() {
  const [agentFilter, setAgentFilter] = useState('');
  const [statusFilter, setStatusFilter] = useState<RunStatus | ''>('');
  const [page, setPage] = useState(0);

  const { data: runs, isLoading, error } = useQuery({
    queryKey: ['runs', agentFilter, statusFilter, page],
    queryFn: () =>
      runsApi.list({
        agent_name: agentFilter || undefined,
        status: (statusFilter as RunStatus) || undefined,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      }),
  });

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-gray-900">Runs</h1>
      </div>

      {/* Filters */}
      <div className="flex gap-3">
        <div className="relative">
          <Search className="absolute left-2.5 top-2 h-4 w-4 text-gray-400" />
          <input
            type="text"
            placeholder="Filter by agent…"
            value={agentFilter}
            onChange={(e) => {
              setAgentFilter(e.target.value);
              setPage(0);
            }}
            className="rounded-md border border-gray-300 py-1.5 pl-8 pr-3 text-sm focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500"
          />
        </div>
        <select
          value={statusFilter}
          onChange={(e) => {
            setStatusFilter(e.target.value as RunStatus | '');
            setPage(0);
          }}
          className="rounded-md border border-gray-300 px-3 py-1.5 text-sm focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500"
        >
          <option value="">All statuses</option>
          <option value="running">Running</option>
          <option value="completed">Completed</option>
          <option value="failed">Failed</option>
          <option value="cancelled">Cancelled</option>
        </select>
      </div>

      {/* Table */}
      <div className="rounded-lg border border-gray-200 bg-white shadow-sm">
        {isLoading ? (
          <div className="flex justify-center py-12">
            <Spinner />
          </div>
        ) : error ? (
          <div className="p-5">
            <ErrorMessage message={extractErrorMessage(error)} />
          </div>
        ) : (
          <>
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
                  <th className="px-5 py-2.5 font-medium">Run ID</th>
                  <th className="px-5 py-2.5 font-medium">Agent</th>
                  <th className="px-5 py-2.5 font-medium">Status</th>
                  <th className="px-5 py-2.5 font-medium">Cost</th>
                  <th className="px-5 py-2.5 font-medium">Tokens</th>
                  <th className="px-5 py-2.5 font-medium">Duration</th>
                  <th className="px-5 py-2.5 font-medium">Started</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-100">
                {(runs ?? []).map((run) => (
                  <tr key={run.id} className="hover:bg-gray-50">
                    <td className="px-5 py-2.5 font-mono text-xs text-gray-500">
                      <Link
                        to={`/runs/${run.id}`}
                        className="text-brand-600 hover:underline"
                      >
                        {run.id.slice(0, 8)}…
                      </Link>
                    </td>
                    <td className="px-5 py-2.5 font-medium text-gray-900">
                      {run.agent_name}
                    </td>
                    <td className="px-5 py-2.5">
                      <RunStatusBadge status={run.status} />
                    </td>
                    <td className="px-5 py-2.5 text-gray-700">{fmt(run.total_cost_usd)}</td>
                    <td className="px-5 py-2.5 text-gray-700">
                      {(run.prompt_tokens + run.completion_tokens).toLocaleString()}
                    </td>
                    <td className="px-5 py-2.5 text-gray-700">{fmtMs(run.duration_ms)}</td>
                    <td className="px-5 py-2.5 text-gray-500 text-xs">
                      {formatDistanceToNow(new Date(run.started_at), { addSuffix: true })}
                    </td>
                  </tr>
                ))}
                {(runs ?? []).length === 0 && (
                  <tr>
                    <td colSpan={7} className="px-5 py-10 text-center text-gray-400">
                      No runs match your filters.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>

            {/* Pagination */}
            <div className="flex items-center justify-between border-t border-gray-200 px-5 py-2.5">
              <p className="text-xs text-gray-500">
                Page {page + 1} — showing {(runs ?? []).length} runs
              </p>
              <div className="flex gap-1">
                <button
                  onClick={() => setPage((p) => Math.max(0, p - 1))}
                  disabled={page === 0}
                  className="rounded p-1 text-gray-600 hover:bg-gray-100 disabled:opacity-40"
                  aria-label="Previous page"
                >
                  <ChevronLeft className="h-4 w-4" />
                </button>
                <button
                  onClick={() => setPage((p) => p + 1)}
                  disabled={(runs ?? []).length < PAGE_SIZE}
                  className="rounded p-1 text-gray-600 hover:bg-gray-100 disabled:opacity-40"
                  aria-label="Next page"
                >
                  <ChevronRight className="h-4 w-4" />
                </button>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
