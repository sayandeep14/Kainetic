/** TypeScript types mirroring the Rust structs in kainetic-cloud. */

export type RunStatus = 'running' | 'completed' | 'failed' | 'cancelled';
export type DeploymentStatus = 'pending' | 'running' | 'stopped' | 'failed';
export type MemberRole = 'viewer' | 'developer' | 'admin';
export type AlertPeriod = 'hourly' | 'daily' | 'monthly';

export interface Run {
  id: string;
  team_id: string;
  agent_id: string | null;
  agent_name: string;
  status: RunStatus;
  input_preview: string | null;
  output_preview: string | null;
  error_message: string | null;
  prompt_tokens: number;
  completion_tokens: number;
  total_cost_usd: number;
  duration_ms: number | null;
  started_at: string;
  completed_at: string | null;
  metadata: Record<string, unknown>;
}

export interface Span {
  id: string;
  run_id: string;
  team_id: string;
  parent_span_id: string | null;
  name: string;
  kind: string;
  status: string;
  start_time: string;
  end_time: string | null;
  attributes: Record<string, unknown>;
  events: SpanEvent[];
}

export interface SpanEvent {
  name: string;
  timestamp: string;
  attributes: Record<string, unknown>;
}

export interface Agent {
  id: string;
  team_id: string;
  name: string;
  version: string;
  description: string | null;
  config: Record<string, unknown>;
  created_at: string;
}

export interface Deployment {
  id: string;
  team_id: string;
  agent_id: string;
  status: DeploymentStatus;
  url: string | null;
  created_at: string;
  updated_at: string;
}

export interface Metrics {
  total_runs: number;
  completed_runs: number;
  failed_runs: number;
  total_cost_usd: number;
  avg_cost_usd: number;
  avg_duration_ms: number;
  total_prompt_tokens: number;
  total_completion_tokens: number;
}

export interface TeamMember {
  user_id: string;
  email: string;
  role: MemberRole;
  joined_at: string;
}

export interface ApiKey {
  id: string;
  key: string;
  prefix: string;
  name: string;
  created_at: string;
}

export interface AuditEntry {
  id: number;
  team_id: string;
  user_id: string | null;
  action: string;
  resource_type: string;
  resource_id: string | null;
  details: Record<string, unknown>;
  ip_address: string | null;
  timestamp: string;
  chain_hash: string;
}

export interface TokenResponse {
  access_token: string;
  token_type: string;
  expires_in: number;
}

export interface RunsFilter {
  agent_name?: string;
  status?: RunStatus;
  limit?: number;
  offset?: number;
}

/** A span tree node — includes all descendant spans inline. */
export interface SpanTreeNode extends Span {
  children: SpanTreeNode[];
  depth: number;
  durationMs: number | null;
}

/** Evaluation test case. */
export interface EvalCase {
  id: string;
  input: string;
  expected_output: string;
  agent_name: string;
}

/** Result of running a single evaluation case. */
export interface EvalResult {
  case_id: string;
  passed: boolean;
  actual_output: string | null;
  score: number | null;
  run_id: string | null;
  error: string | null;
}

/** A stored system prompt version. */
export interface PromptVersion {
  id: string;
  agent_name: string;
  version: number;
  content: string;
  created_at: string;
  is_active: boolean;
}

export interface CostAlertConfig {
  id: string;
  team_id: string;
  agent_name: string | null;
  threshold_usd: number;
  period: AlertPeriod;
  webhook_url: string | null;
  notification_email: string | null;
  created_at: string;
}
