import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Layout } from './components/Layout';
import { RequireAuth } from './components/RequireAuth';
import { LoginPage } from './pages/LoginPage';
import { OverviewPage } from './pages/OverviewPage';
import { RunsPage } from './pages/RunsPage';
import { RunDetailPage } from './pages/RunDetailPage';
import { CostPage } from './pages/CostPage';
import { LatencyPage } from './pages/LatencyPage';
import { AgentsPage } from './pages/AgentsPage';
import { EvaluationsPage } from './pages/EvaluationsPage';
import { PromptsPage } from './pages/PromptsPage';
import { TeamPage } from './pages/TeamPage';
import { BillingPage } from './pages/BillingPage';

const qc = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
    },
  },
});

export function App() {
  return (
    <QueryClientProvider client={qc}>
      <BrowserRouter>
        <Routes>
          <Route path="/login" element={<LoginPage />} />

          <Route
            element={
              <RequireAuth>
                <Layout />
              </RequireAuth>
            }
          >
            <Route index element={<OverviewPage />} />
            <Route path="runs" element={<RunsPage />} />
            <Route path="runs/:id" element={<RunDetailPage />} />
            <Route path="cost" element={<CostPage />} />
            <Route path="latency" element={<LatencyPage />} />
            <Route path="agents" element={<AgentsPage />} />
            <Route path="evaluations" element={<EvaluationsPage />} />
            <Route path="prompts" element={<PromptsPage />} />
            <Route path="team" element={<TeamPage />} />
            <Route path="billing" element={<BillingPage />} />
          </Route>

          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
