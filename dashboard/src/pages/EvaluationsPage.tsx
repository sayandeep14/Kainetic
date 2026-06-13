import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { agentsApi } from '../api/agents';
import { runsApi } from '../api/runs';
import type { EvalCase, EvalResult } from '../api/types';
import { Spinner } from '../components/Spinner';
import { CheckCircle, XCircle, FlaskConical } from 'lucide-react';

/** In-memory eval store (a real implementation would persist to the cloud backend). */
let _cases: EvalCase[] = [];
let _results: EvalResult[] = [];

export function EvaluationsPage() {
  const [cases, setCases] = useState<EvalCase[]>(_cases);
  const [results, setResults] = useState<EvalResult[]>(_results);
  const [selectedAgent, setSelectedAgent] = useState('');
  const [input, setInput] = useState('');
  const [expected, setExpected] = useState('');
  const [running, setRunning] = useState(false);

  const agentsQ = useQuery({ queryKey: ['agents'], queryFn: agentsApi.list });

  const addCase = () => {
    if (!input.trim() || !expected.trim() || !selectedAgent) return;
    const c: EvalCase = {
      id: crypto.randomUUID(),
      input,
      expected_output: expected,
      agent_name: selectedAgent,
    };
    const next = [...cases, c];
    setCases(next);
    _cases = next;
    setInput('');
    setExpected('');
  };

  const runEval = async () => {
    if (cases.length === 0) return;
    setRunning(true);
    const newResults: EvalResult[] = [];
    for (const c of cases) {
      try {
        const runs = await runsApi.list({ agent_name: c.agent_name, limit: 1 });
        const lastRun = runs[0];
        const actual = lastRun?.output_preview ?? null;
        const passed = actual !== null && actual.includes(c.expected_output);
        newResults.push({
          case_id: c.id,
          passed,
          actual_output: actual,
          score: passed ? 1.0 : 0.0,
          run_id: lastRun?.id ?? null,
          error: null,
        });
      } catch {
        newResults.push({
          case_id: c.id,
          passed: false,
          actual_output: null,
          score: 0,
          run_id: null,
          error: 'Failed to fetch run data',
        });
      }
    }
    setResults(newResults);
    _results = newResults;
    setRunning(false);
  };

  const passCount = results.filter((r) => r.passed).length;

  return (
    <div className="p-6 space-y-5">
      <h1 className="text-xl font-semibold text-gray-900">Evaluations</h1>

      {/* Add test case */}
      <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm space-y-4">
        <h2 className="text-sm font-semibold text-gray-900">Add Test Case</h2>
        <div className="grid gap-3 sm:grid-cols-3">
          <div>
            <label className="block text-xs font-medium text-gray-600 mb-1">Agent</label>
            <select
              value={selectedAgent}
              onChange={(e) => setSelectedAgent(e.target.value)}
              className="w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm focus:border-brand-500 focus:outline-none"
            >
              <option value="">Select…</option>
              {(agentsQ.data ?? []).map((a) => (
                <option key={a.id} value={a.name}>
                  {a.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="block text-xs font-medium text-gray-600 mb-1">Input</label>
            <input
              value={input}
              onChange={(e) => setInput(e.target.value)}
              className="w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm focus:border-brand-500 focus:outline-none"
              placeholder="What is the capital of France?"
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-gray-600 mb-1">
              Expected output (substring)
            </label>
            <input
              value={expected}
              onChange={(e) => setExpected(e.target.value)}
              className="w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm focus:border-brand-500 focus:outline-none"
              placeholder="Paris"
            />
          </div>
        </div>
        <button
          onClick={addCase}
          disabled={!input.trim() || !expected.trim() || !selectedAgent}
          className="rounded-md bg-gray-100 px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-200 disabled:opacity-50"
        >
          Add case
        </button>
      </div>

      {/* Test cases table */}
      {cases.length > 0 && (
        <div className="rounded-lg border border-gray-200 bg-white shadow-sm">
          <div className="flex items-center justify-between border-b border-gray-200 px-5 py-3">
            <h2 className="text-sm font-semibold text-gray-900">
              Test Suite ({cases.length} cases)
            </h2>
            <button
              onClick={() => void runEval()}
              disabled={running}
              className="flex items-center gap-1.5 rounded-md bg-brand-600 px-3 py-1.5 text-sm font-semibold text-white hover:bg-brand-700 disabled:opacity-60"
            >
              {running ? <Spinner size="sm" /> : <FlaskConical className="h-4 w-4" />}
              Run evaluation
            </button>
          </div>

          {results.length > 0 && (
            <div className="border-b border-gray-200 px-5 py-2.5 text-sm text-gray-700">
              Score:{' '}
              <span className={passCount === cases.length ? 'text-green-600 font-semibold' : 'text-red-600 font-semibold'}>
                {passCount}/{cases.length}
              </span>{' '}
              ({((passCount / cases.length) * 100).toFixed(0)}%)
            </div>
          )}

          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
                <th className="px-5 py-2 font-medium">Agent</th>
                <th className="px-5 py-2 font-medium">Input</th>
                <th className="px-5 py-2 font-medium">Expected</th>
                <th className="px-5 py-2 font-medium">Result</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100">
              {cases.map((c) => {
                const result = results.find((r) => r.case_id === c.id);
                return (
                  <tr key={c.id}>
                    <td className="px-5 py-2.5 text-gray-700">{c.agent_name}</td>
                    <td className="px-5 py-2.5 font-mono text-xs text-gray-600">{c.input}</td>
                    <td className="px-5 py-2.5 font-mono text-xs text-gray-600">
                      {c.expected_output}
                    </td>
                    <td className="px-5 py-2.5">
                      {result ? (
                        result.passed ? (
                          <CheckCircle className="h-4 w-4 text-green-500" />
                        ) : (
                          <XCircle className="h-4 w-4 text-red-500" />
                        )
                      ) : (
                        <span className="text-xs text-gray-400">—</span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {cases.length === 0 && (
        <p className="text-center text-sm text-gray-400 py-6">
          Add test cases above to build your evaluation suite.
        </p>
      )}
    </div>
  );
}
