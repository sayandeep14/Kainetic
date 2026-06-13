import { useState } from 'react';
import { useParams, Link } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { runsApi, spansApi } from '../api/runs';
import type { SpanTreeNode } from '../api/types';
import { RunStatusBadge } from '../components/RunStatusBadge';
import { Spinner } from '../components/Spinner';
import { ErrorMessage } from '../components/ErrorMessage';
import { extractErrorMessage } from '../api/client';
import { ChevronRight, ChevronDown, Clock } from 'lucide-react';
import { format } from 'date-fns';

function SpanNode({ node, totalMs }: { node: SpanTreeNode; totalMs: number }) {
  const [expanded, setExpanded] = useState(node.depth < 2);
  const indent = node.depth * 20;
  const widthPct =
    totalMs > 0 && node.durationMs !== null ? (node.durationMs / totalMs) * 100 : null;

  const statusColor =
    node.status === 'ok' ? 'bg-green-400' : node.status === 'error' ? 'bg-red-400' : 'bg-yellow-400';

  return (
    <li>
      <button
        onClick={() => setExpanded((e) => !e)}
        className="flex w-full items-start gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-gray-50"
        style={{ paddingLeft: `${indent + 12}px` }}
      >
        {node.children.length > 0 ? (
          expanded ? (
            <ChevronDown className="mt-0.5 h-3.5 w-3.5 flex-shrink-0 text-gray-400" />
          ) : (
            <ChevronRight className="mt-0.5 h-3.5 w-3.5 flex-shrink-0 text-gray-400" />
          )
        ) : (
          <span className="mt-0.5 h-3.5 w-3.5 flex-shrink-0" />
        )}

        <span className={`mt-1.5 h-2 w-2 flex-shrink-0 rounded-full ${statusColor}`} />

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate font-medium text-gray-900">{node.name}</span>
            <span className="flex-shrink-0 rounded bg-gray-100 px-1.5 py-0.5 text-xs text-gray-500">
              {node.kind}
            </span>
          </div>

          {widthPct !== null && (
            <div className="mt-1 h-1 w-full rounded-full bg-gray-100">
              <div
                className="h-1 rounded-full bg-brand-400"
                style={{ width: `${Math.max(widthPct, 1)}%` }}
              />
            </div>
          )}

          <div className="mt-0.5 flex items-center gap-1 text-xs text-gray-400">
            <Clock className="h-3 w-3" />
            {node.durationMs !== null ? `${node.durationMs}ms` : 'in flight'}
          </div>
        </div>
      </button>

      {/* Attributes panel */}
      {expanded && Object.keys(node.attributes).length > 0 && (
        <div
          className="mb-1 rounded-md bg-gray-50 p-2 font-mono text-xs text-gray-600"
          style={{ marginLeft: `${indent + 36}px` }}
        >
          {Object.entries(node.attributes).map(([k, v]) => (
            <div key={k}>
              <span className="text-gray-400">{k}: </span>
              <span>{JSON.stringify(v)}</span>
            </div>
          ))}
        </div>
      )}

      {/* Children */}
      {expanded && node.children.length > 0 && (
        <ul>
          {node.children.map((child) => (
            <SpanNode key={child.id} node={child} totalMs={totalMs} />
          ))}
        </ul>
      )}
    </li>
  );
}

export function RunDetailPage() {
  const { id } = useParams<{ id: string }>();

  const runQ = useQuery({
    queryKey: ['run', id],
    queryFn: () => runsApi.get(id!),
    enabled: !!id,
  });

  const spansQ = useQuery({
    queryKey: ['spans', id],
    queryFn: () => spansApi.getTree(id!),
    enabled: !!id,
  });

  const run = runQ.data;
  const totalMs = run?.duration_ms ?? 0;

  return (
    <div className="p-6 space-y-5">
      <div className="flex items-center gap-2 text-sm text-gray-500">
        <Link to="/runs" className="hover:text-brand-600">
          Runs
        </Link>
        <span>/</span>
        <span className="font-mono text-gray-700">{id?.slice(0, 8)}…</span>
      </div>

      {runQ.isLoading ? (
        <div className="flex justify-center py-12">
          <Spinner />
        </div>
      ) : runQ.error ? (
        <ErrorMessage message={extractErrorMessage(runQ.error)} />
      ) : run ? (
        <>
          {/* Run header */}
          <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
            <div className="flex items-start justify-between">
              <div>
                <h1 className="text-xl font-semibold text-gray-900">{run.agent_name}</h1>
                <p className="mt-0.5 font-mono text-xs text-gray-400">{run.id}</p>
              </div>
              <RunStatusBadge status={run.status} />
            </div>

            <div className="mt-4 grid grid-cols-2 gap-4 text-sm sm:grid-cols-4">
              <div>
                <p className="text-xs text-gray-500">Cost</p>
                <p className="font-medium text-gray-900">
                  ${run.total_cost_usd.toFixed(6)}
                </p>
              </div>
              <div>
                <p className="text-xs text-gray-500">Duration</p>
                <p className="font-medium text-gray-900">
                  {run.duration_ms !== null ? `${run.duration_ms}ms` : '—'}
                </p>
              </div>
              <div>
                <p className="text-xs text-gray-500">Tokens (P / C)</p>
                <p className="font-medium text-gray-900">
                  {run.prompt_tokens.toLocaleString()} / {run.completion_tokens.toLocaleString()}
                </p>
              </div>
              <div>
                <p className="text-xs text-gray-500">Started</p>
                <p className="font-medium text-gray-900">
                  {format(new Date(run.started_at), 'MMM d, HH:mm:ss')}
                </p>
              </div>
            </div>

            {run.error_message && (
              <div className="mt-4 rounded-md bg-red-50 p-3 text-sm text-red-700">
                {run.error_message}
              </div>
            )}

            {(run.input_preview || run.output_preview) && (
              <div className="mt-4 grid gap-3 sm:grid-cols-2">
                {run.input_preview && (
                  <div>
                    <p className="mb-1 text-xs font-medium text-gray-500">Input preview</p>
                    <pre className="rounded-md bg-gray-50 p-3 text-xs text-gray-700 overflow-auto max-h-24 font-mono">
                      {run.input_preview}
                    </pre>
                  </div>
                )}
                {run.output_preview && (
                  <div>
                    <p className="mb-1 text-xs font-medium text-gray-500">Output preview</p>
                    <pre className="rounded-md bg-gray-50 p-3 text-xs text-gray-700 overflow-auto max-h-24 font-mono">
                      {run.output_preview}
                    </pre>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Trace tree */}
          <div className="rounded-lg border border-gray-200 bg-white shadow-sm">
            <div className="border-b border-gray-200 px-5 py-3">
              <h2 className="text-sm font-semibold text-gray-900">Trace</h2>
            </div>
            {spansQ.isLoading ? (
              <div className="flex justify-center py-8">
                <Spinner />
              </div>
            ) : spansQ.error ? (
              <div className="p-5">
                <ErrorMessage message={extractErrorMessage(spansQ.error)} />
              </div>
            ) : (spansQ.data ?? []).length === 0 ? (
              <p className="px-5 py-8 text-center text-sm text-gray-400">No spans recorded.</p>
            ) : (
              <ul className="py-2">
                {(spansQ.data ?? []).map((root) => (
                  <SpanNode key={root.id} node={root} totalMs={totalMs} />
                ))}
              </ul>
            )}
          </div>
        </>
      ) : null}
    </div>
  );
}
