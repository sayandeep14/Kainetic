import { useQuery } from '@tanstack/react-query';
import { metricsApi } from '../api/metrics';
import { StatCard } from '../components/StatCard';
import { Spinner } from '../components/Spinner';
import { extractErrorMessage } from '../api/client';
import { ErrorMessage } from '../components/ErrorMessage';

const PLAN_LIMIT_USD = 100;

export function BillingPage() {
  const metricsQ = useQuery({ queryKey: ['metrics'], queryFn: metricsApi.get });

  const spent = metricsQ.data?.total_cost_usd ?? 0;
  const pct = Math.min((spent / PLAN_LIMIT_USD) * 100, 100);

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-xl font-semibold text-gray-900">Billing</h1>

      {/* Plan banner */}
      <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-xs font-medium text-gray-500 uppercase tracking-wide">Current Plan</p>
            <p className="mt-0.5 text-lg font-semibold text-gray-900">Developer</p>
          </div>
          <button className="rounded-md border border-brand-300 px-4 py-1.5 text-sm font-semibold text-brand-700 hover:bg-brand-50">
            Upgrade
          </button>
        </div>

        <div className="mt-4">
          <div className="flex items-end justify-between text-sm">
            <span className="text-gray-600">Usage this period</span>
            <span className="font-medium text-gray-900">
              ${spent.toFixed(4)} / ${PLAN_LIMIT_USD.toFixed(2)}
            </span>
          </div>
          <div className="mt-1.5 h-2 w-full rounded-full bg-gray-200">
            <div
              className={`h-2 rounded-full transition-all ${
                pct > 80 ? 'bg-red-500' : pct > 50 ? 'bg-yellow-400' : 'bg-brand-500'
              }`}
              style={{ width: `${pct}%` }}
            />
          </div>
          {pct > 80 && (
            <p className="mt-1 text-xs text-red-600">
              ⚠ You're using {pct.toFixed(0)}% of your monthly budget.
            </p>
          )}
        </div>
      </div>

      {/* Stats */}
      {metricsQ.isLoading ? (
        <div className="flex justify-center py-8">
          <Spinner />
        </div>
      ) : metricsQ.error ? (
        <ErrorMessage message={extractErrorMessage(metricsQ.error)} />
      ) : metricsQ.data ? (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
          <StatCard
            title="Total Spend"
            value={`$${metricsQ.data.total_cost_usd.toFixed(4)}`}
          />
          <StatCard
            title="Total Runs"
            value={metricsQ.data.total_runs.toLocaleString()}
          />
          <StatCard
            title="Prompt Tokens"
            value={metricsQ.data.total_prompt_tokens.toLocaleString()}
          />
          <StatCard
            title="Completion Tokens"
            value={metricsQ.data.total_completion_tokens.toLocaleString()}
          />
        </div>
      ) : null}

      {/* Invoice placeholder */}
      <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
        <h2 className="mb-3 text-sm font-semibold text-gray-900">Invoices</h2>
        <p className="text-sm text-gray-400">
          No invoices yet. Your first invoice will appear here at the end of your billing cycle.
        </p>
      </div>
    </div>
  );
}
