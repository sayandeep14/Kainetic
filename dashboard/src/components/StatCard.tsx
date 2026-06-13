interface Props {
  title: string;
  value: string | number;
  sub?: string;
  trend?: 'up' | 'down' | 'neutral';
}

export function StatCard({ title, value, sub, trend }: Props) {
  const trendColor =
    trend === 'up' ? 'text-green-600' : trend === 'down' ? 'text-red-600' : 'text-gray-500';

  return (
    <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
      <p className="text-sm text-gray-500">{title}</p>
      <p className="mt-1 text-2xl font-semibold text-gray-900">{value}</p>
      {sub && <p className={`mt-0.5 text-xs ${trendColor}`}>{sub}</p>}
    </div>
  );
}
