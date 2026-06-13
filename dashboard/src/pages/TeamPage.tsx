import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { teamsApi } from '../api/teams';
import { useAuthStore } from '../store/authStore';
import { Spinner } from '../components/Spinner';
import { ErrorMessage } from '../components/ErrorMessage';
import { extractErrorMessage } from '../api/client';
import type { ApiKey } from '../api/types';
import { Copy, Plus, Key, Users, ScrollText } from 'lucide-react';
import { format } from 'date-fns';

function copyToClipboard(text: string) {
  void navigator.clipboard.writeText(text);
}

export function TeamPage() {
  const teamId = useAuthStore((s) => s.teamId) ?? '';
  const [newKeyName, setNewKeyName] = useState('');
  const [revealedKey, setRevealedKey] = useState<ApiKey | null>(null);
  const [tab, setTab] = useState<'members' | 'keys' | 'audit'>('members');
  const qc = useQueryClient();

  const membersQ = useQuery({
    queryKey: ['members', teamId],
    queryFn: () => teamsApi.listMembers(teamId),
    enabled: !!teamId,
  });

  const auditQ = useQuery({
    queryKey: ['audit'],
    queryFn: teamsApi.listAudit,
    enabled: tab === 'audit',
  });

  const createKeyMutation = useMutation({
    mutationFn: () => teamsApi.createApiKey(teamId, newKeyName),
    onSuccess: (key) => {
      setRevealedKey(key);
      setNewKeyName('');
      void qc.invalidateQueries({ queryKey: ['members', teamId] });
    },
  });

  const tabs = [
    { id: 'members' as const, label: 'Members', icon: Users },
    { id: 'keys' as const, label: 'API Keys', icon: Key },
    { id: 'audit' as const, label: 'Audit Log', icon: ScrollText },
  ];

  return (
    <div className="p-6 space-y-5">
      <h1 className="text-xl font-semibold text-gray-900">Team Settings</h1>

      {/* Tabs */}
      <div className="flex gap-1 border-b border-gray-200">
        {tabs.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            className={`flex items-center gap-1.5 px-4 py-2 text-sm font-medium transition-colors ${
              tab === id
                ? 'border-b-2 border-brand-600 text-brand-700'
                : 'text-gray-500 hover:text-gray-700'
            }`}
          >
            <Icon className="h-4 w-4" />
            {label}
          </button>
        ))}
      </div>

      {/* Members */}
      {tab === 'members' && (
        <div className="rounded-lg border border-gray-200 bg-white shadow-sm">
          {membersQ.isLoading ? (
            <div className="flex justify-center py-8">
              <Spinner />
            </div>
          ) : membersQ.error ? (
            <div className="p-5">
              <ErrorMessage message={extractErrorMessage(membersQ.error)} />
            </div>
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
                  <th className="px-5 py-2.5 font-medium">Email</th>
                  <th className="px-5 py-2.5 font-medium">Role</th>
                  <th className="px-5 py-2.5 font-medium">Joined</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-100">
                {(membersQ.data ?? []).map((m) => (
                  <tr key={m.user_id}>
                    <td className="px-5 py-2.5 text-gray-900">{m.email}</td>
                    <td className="px-5 py-2.5">
                      <span className="rounded-full bg-gray-100 px-2.5 py-0.5 text-xs capitalize text-gray-700">
                        {m.role}
                      </span>
                    </td>
                    <td className="px-5 py-2.5 text-gray-500 text-xs">
                      {format(new Date(m.joined_at), 'MMM d, yyyy')}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}

      {/* API Keys */}
      {tab === 'keys' && (
        <div className="space-y-4">
          {/* Revealed key banner */}
          {revealedKey && (
            <div className="rounded-lg border border-green-300 bg-green-50 p-4">
              <p className="mb-1 text-sm font-semibold text-green-800">
                Key created — copy it now, it won't be shown again.
              </p>
              <div className="flex items-center gap-2">
                <code className="flex-1 rounded bg-white px-3 py-1.5 font-mono text-sm text-green-900 border border-green-200">
                  {revealedKey.key}
                </code>
                <button
                  onClick={() => copyToClipboard(revealedKey.key)}
                  className="rounded-md border border-green-300 p-1.5 text-green-700 hover:bg-green-100"
                  aria-label="Copy key"
                >
                  <Copy className="h-4 w-4" />
                </button>
              </div>
              <button
                onClick={() => setRevealedKey(null)}
                className="mt-2 text-xs text-green-600 hover:underline"
              >
                Dismiss
              </button>
            </div>
          )}

          {/* Create key form */}
          <div className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
            <h2 className="mb-3 text-sm font-semibold text-gray-900">Create API Key</h2>
            <div className="flex gap-2">
              <input
                value={newKeyName}
                onChange={(e) => setNewKeyName(e.target.value)}
                placeholder="Key name (e.g. CI deploy)"
                className="flex-1 rounded-md border border-gray-300 px-3 py-1.5 text-sm focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500"
              />
              <button
                onClick={() => createKeyMutation.mutate()}
                disabled={createKeyMutation.isPending || !newKeyName.trim()}
                className="flex items-center gap-1.5 rounded-md bg-brand-600 px-3 py-1.5 text-sm font-semibold text-white hover:bg-brand-700 disabled:opacity-60"
              >
                {createKeyMutation.isPending ? <Spinner size="sm" /> : <Plus className="h-4 w-4" />}
                Create
              </button>
            </div>
            {createKeyMutation.error && (
              <p className="mt-2 text-sm text-red-600">
                {extractErrorMessage(createKeyMutation.error)}
              </p>
            )}
          </div>
        </div>
      )}

      {/* Audit log */}
      {tab === 'audit' && (
        <div className="rounded-lg border border-gray-200 bg-white shadow-sm">
          {auditQ.isLoading ? (
            <div className="flex justify-center py-8">
              <Spinner />
            </div>
          ) : auditQ.error ? (
            <div className="p-5">
              <ErrorMessage message={extractErrorMessage(auditQ.error)} />
            </div>
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
                  <th className="px-5 py-2.5 font-medium">#</th>
                  <th className="px-5 py-2.5 font-medium">Action</th>
                  <th className="px-5 py-2.5 font-medium">Resource</th>
                  <th className="px-5 py-2.5 font-medium">IP</th>
                  <th className="px-5 py-2.5 font-medium">Time</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-100">
                {(auditQ.data ?? []).map((e) => (
                  <tr key={e.id}>
                    <td className="px-5 py-2.5 font-mono text-xs text-gray-400">{e.id}</td>
                    <td className="px-5 py-2.5 font-mono text-xs text-gray-700">{e.action}</td>
                    <td className="px-5 py-2.5 text-xs text-gray-600">
                      {e.resource_type}
                      {e.resource_id && (
                        <span className="ml-1 text-gray-400">({e.resource_id.slice(0, 8)}…)</span>
                      )}
                    </td>
                    <td className="px-5 py-2.5 text-xs text-gray-500">{e.ip_address ?? '—'}</td>
                    <td className="px-5 py-2.5 text-xs text-gray-500">
                      {format(new Date(e.timestamp), 'MMM d, HH:mm:ss')}
                    </td>
                  </tr>
                ))}
                {(auditQ.data ?? []).length === 0 && (
                  <tr>
                    <td colSpan={5} className="px-5 py-8 text-center text-gray-400">
                      No audit events yet.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          )}
        </div>
      )}
    </div>
  );
}
