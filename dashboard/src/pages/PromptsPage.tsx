import { useState } from 'react';
import type { PromptVersion } from '../api/types';
import { format } from 'date-fns';
import { Plus, Check } from 'lucide-react';

/** In-memory prompt store (a real impl would persist to /v1/prompts). */
let _prompts: PromptVersion[] = [
  {
    id: crypto.randomUUID(),
    agent_name: 'assistant',
    version: 1,
    content: 'You are a helpful AI assistant. Answer concisely and accurately.',
    created_at: new Date().toISOString(),
    is_active: true,
  },
];

export function PromptsPage() {
  const [prompts, setPrompts] = useState<PromptVersion[]>(_prompts);
  const [selectedAgent, setSelectedAgent] = useState(
    prompts[0]?.agent_name ?? '',
  );
  const [draft, setDraft] = useState('');
  const [showNew, setShowNew] = useState(false);
  const [newAgent, setNewAgent] = useState('');

  const agentNames = [...new Set(prompts.map((p) => p.agent_name))];
  const agentPrompts = prompts.filter((p) => p.agent_name === selectedAgent).sort(
    (a, b) => b.version - a.version,
  );
  const activePrompt = agentPrompts.find((p) => p.is_active);

  const createVersion = () => {
    if (!draft.trim()) return;
    const next: PromptVersion = {
      id: crypto.randomUUID(),
      agent_name: selectedAgent,
      version: (agentPrompts[0]?.version ?? 0) + 1,
      content: draft,
      created_at: new Date().toISOString(),
      is_active: false,
    };
    const updated = [...prompts, next];
    setPrompts(updated);
    _prompts = updated;
    setDraft('');
  };

  const activate = (id: string) => {
    const updated = prompts.map((p) =>
      p.agent_name === selectedAgent ? { ...p, is_active: p.id === id } : p,
    );
    setPrompts(updated);
    _prompts = updated;
  };

  const addAgent = () => {
    if (!newAgent.trim()) return;
    const next: PromptVersion = {
      id: crypto.randomUUID(),
      agent_name: newAgent.trim(),
      version: 1,
      content: '',
      created_at: new Date().toISOString(),
      is_active: true,
    };
    const updated = [...prompts, next];
    setPrompts(updated);
    _prompts = updated;
    setSelectedAgent(newAgent.trim());
    setNewAgent('');
    setShowNew(false);
  };

  return (
    <div className="p-6 space-y-5">
      <h1 className="text-xl font-semibold text-gray-900">Prompts</h1>

      <div className="flex gap-4">
        {/* Agent selector */}
        <div className="w-48 flex-shrink-0 space-y-1">
          {agentNames.map((name) => (
            <button
              key={name}
              onClick={() => setSelectedAgent(name)}
              className={`w-full rounded-md px-3 py-1.5 text-left text-sm font-medium transition-colors ${
                selectedAgent === name
                  ? 'bg-brand-50 text-brand-700'
                  : 'text-gray-600 hover:bg-gray-100'
              }`}
            >
              {name}
            </button>
          ))}
          {showNew ? (
            <div className="flex gap-1 pt-1">
              <input
                value={newAgent}
                onChange={(e) => setNewAgent(e.target.value)}
                placeholder="agent-name"
                className="flex-1 rounded-md border border-gray-300 px-2 py-1 text-xs"
                onKeyDown={(e) => e.key === 'Enter' && addAgent()}
              />
              <button onClick={addAgent} className="rounded-md bg-brand-600 px-2 py-1 text-xs text-white">
                Add
              </button>
            </div>
          ) : (
            <button
              onClick={() => setShowNew(true)}
              className="flex w-full items-center gap-1 rounded-md px-3 py-1.5 text-sm text-gray-400 hover:bg-gray-100"
            >
              <Plus className="h-3.5 w-3.5" />
              Add agent
            </button>
          )}
        </div>

        {/* Prompt versions */}
        {selectedAgent && (
          <div className="flex-1 space-y-4">
            {/* New draft */}
            <div className="rounded-lg border border-gray-200 bg-white p-4 shadow-sm">
              <h3 className="mb-2 text-sm font-medium text-gray-700">New version</h3>
              <textarea
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                rows={4}
                placeholder="Enter new system prompt…"
                className="w-full rounded-md border border-gray-300 px-3 py-2 font-mono text-sm focus:border-brand-500 focus:outline-none"
              />
              <button
                onClick={createVersion}
                disabled={!draft.trim()}
                className="mt-2 rounded-md bg-brand-600 px-3 py-1.5 text-sm font-semibold text-white hover:bg-brand-700 disabled:opacity-50"
              >
                Save version
              </button>
            </div>

            {/* Version history */}
            <div className="space-y-2">
              {agentPrompts.map((p) => (
                <div
                  key={p.id}
                  className={`rounded-lg border p-4 ${
                    p.is_active ? 'border-brand-300 bg-brand-50' : 'border-gray-200 bg-white'
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-semibold text-gray-500">v{p.version}</span>
                      {p.is_active && (
                        <span className="flex items-center gap-1 rounded-full bg-brand-100 px-2 py-0.5 text-xs text-brand-700">
                          <Check className="h-3 w-3" />
                          Active
                        </span>
                      )}
                    </div>
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-gray-400">
                        {format(new Date(p.created_at), 'MMM d, HH:mm')}
                      </span>
                      {!p.is_active && (
                        <button
                          onClick={() => activate(p.id)}
                          className="rounded-md border border-gray-300 px-2 py-0.5 text-xs text-gray-600 hover:bg-gray-50"
                        >
                          Activate
                        </button>
                      )}
                    </div>
                  </div>
                  <pre className="mt-2 whitespace-pre-wrap font-mono text-xs text-gray-700">
                    {p.content || <span className="italic text-gray-400">(empty)</span>}
                  </pre>

                  {/* Diff vs previous */}
                  {agentPrompts.findIndex((x) => x.id === p.id) < agentPrompts.length - 1 && (
                    <details className="mt-2">
                      <summary className="cursor-pointer text-xs text-gray-400 hover:text-gray-600">
                        Show diff vs previous
                      </summary>
                      <div className="mt-1 rounded bg-gray-100 p-2 font-mono text-xs text-gray-600">
                        {/* Simple line diff indication */}
                        {p.content !== activePrompt?.content
                          ? '~ content changed'
                          : '= same as active'}
                      </div>
                    </details>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
