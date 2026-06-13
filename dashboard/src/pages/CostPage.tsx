import { useQuery } from '@tanstack/react-query';
import { runsApi } from '../api/runs';
import { metricsApi } from '../api/metrics';
import { StatCard } from '../components/StatCard';
import { Spinner } from '../components/Spinner';
import { ErrorMessage } from '../components/ErrorMessage';
import { extractErrorMessage } from '../api/client';
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  PieChart,
  Pie,
  Cell,
  Legend,
} from 'recharts';
import { format } from 'date-fns';

const COLORS = ['#3d57f5', '#6282fc', '#93aeff', '#bfd0ff', '#dce6ff'];

export function CostPage() {
  const metricsQ = useQuery({ queryKey: ['metrics'], queryFn: metricsApi.get });
  const runsQ = useQuery({
    queryKey: ['runs', 'cost'],
    queryFn: () => runsApi.list({ limit: 200 }),
  });

  // Daily cost aggregation.
  const dailyCost = (() => {
    if (!runsQ.data) return [];
    const agg = new Map<string, number>();
    for (const r of runsQ.data) {
      const day = format(new Date(r.started_at), 'MMM d');
      agg.set(day, (agg.get(day) ?? 0) + r.total_cost_usd);
    }
    return Array.from(agg.entries()).map(([day, cost]) => ({ day, cost: +cost.toFixed(6) }));
  })();

  // Cost by agent.
  const costByAgent = (() => {
    if (!runsQ.data) return [];
    const agg = new Map<string, number>();
    for (const r of runsQ.data) {
      agg.set(r.agent_name, (agg.get(r.agent_name) ?? 0) + r.total_cost_usd);
    }
    return Array.from(agg.entries())
      .map(([name, value]) => ({ name, value: +value.toFixed(6) }))
      .sort((a, b) => b.value - a.value);
  })();

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-xl font-semibold text-gray-900">Cost</h1>

      {metricsQ.isLoading ? (
        <div className="flex justify-center py-8">
          <Spinner />
        </div>
      ) : metricsQ.error ? (
        <ErrorMessage message={extractErrorMessage(metricsQ.error)} />
      ) : metricsQ.data ? (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3">
          <StatCard
            title="Total Spend"
            value={`$${metricsQ.data.total_cost_usd.toFixed(4)}`}
          />
          <StatCard
            title="Avg Cost / Run"
            value={`$${metricsQ.data.avg_cost_usd.toFixed(6)}`}
          />
          <StatCard
            title="Total Tokens"
            value={(
              metricsQ.data.total_prompt_tokens + metricsQ.data.total_completion_tokens
            ).toLocaleString()}
          />
        </div>
      ) : null}

      {runsQ.isLoading ? (
        <div className="flex justify-center py-8">
          <Spinner />
        </div>
      ) : runsQ.error ? (
        <ErrorMessage message={extractErrorMessage(runsQ.error)} />
      ) : (
        <div className="grid gap-6 lg:grid-cols-2">
          {/* Daily cost bar chart */}
          <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
            <h2 className="mb-4 text-sm font-semibold text-gray-900">Daily Spend</h2>
            {dailyCost.length === 0 ? (
              <p className="py-8 text-center text-sm text-gray-400">No data.</p>
            ) : (
              <ResponsiveContainer width="100%" height={220}>
                <BarChart data={dailyCost}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#f0f0f0" />
                  <XAxis dataKey="day" tick={{ fontSize: 11 }} />
                  <YAxis
                    tick={{ fontSize: 11 }}
                    tickFormatter={(v: number) => `$${v.toFixed(4)}`}
                  />
                  <Tooltip formatter={(v: number) => [`$${v.toFixed(6)}`, 'Cost']} />
                  <Bar dataKey="cost" fill="#3d57f5" radius={[3, 3, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            )}
          </div>

          {/* Cost by agent pie */}
          <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
            <h2 className="mb-4 text-sm font-semibold text-gray-900">Cost by Agent</h2>
            {costByAgent.length === 0 ? (
              <p className="py-8 text-center text-sm text-gray-400">No data.</p>
            ) : (
              <ResponsiveContainer width="100%" height={220}>
                <PieChart>
                  <Pie
                    data={costByAgent}
                    dataKey="value"
                    nameKey="name"
                    cx="50%"
                    cy="50%"
                    outerRadius={80}
                    label={({ name, percent }) =>
                      `${name} ${((percent as number) * 100).toFixed(0)}%`
                    }
                    labelLine={false}
                  >
                    {costByAgent.map((_, i) => (
                      <Cell key={i} fill={COLORS[i % COLORS.length]} />
                    ))}
                  </Pie>
                  <Tooltip formatter={(v: number) => [`$${v.toFixed(6)}`, 'Cost']} />
                  <Legend />
                </PieChart>
              </ResponsiveContainer>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
