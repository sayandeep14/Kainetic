import { apiClient } from './client';
import type { Run, Span, RunsFilter, SpanTreeNode } from './types';

export const runsApi = {
  list: async (filter?: RunsFilter): Promise<Run[]> => {
    const { data } = await apiClient.get<Run[]>('/runs', { params: filter });
    return data;
  },

  get: async (id: string): Promise<Run> => {
    const { data } = await apiClient.get<Run>(`/runs/${id}`);
    return data;
  },
};

export const spansApi = {
  /** Fetches all spans for a run and returns them as a tree. */
  getTree: async (runId: string): Promise<SpanTreeNode[]> => {
    const { data } = await apiClient.get<Span[]>(`/runs/${runId}/spans`);
    return buildSpanTree(data);
  },
};

/** Converts a flat span array into a tree by parent_span_id. */
export function buildSpanTree(spans: Span[]): SpanTreeNode[] {
  const byId = new Map<string, SpanTreeNode>();
  const roots: SpanTreeNode[] = [];

  for (const span of spans) {
    byId.set(span.id, {
      ...span,
      children: [],
      depth: 0,
      durationMs: span.end_time
        ? new Date(span.end_time).getTime() - new Date(span.start_time).getTime()
        : null,
    });
  }

  for (const node of byId.values()) {
    if (node.parent_span_id && byId.has(node.parent_span_id)) {
      byId.get(node.parent_span_id)!.children.push(node);
    } else {
      roots.push(node);
    }
  }

  // Assign depth recursively.
  const assignDepth = (nodes: SpanTreeNode[], depth: number) => {
    for (const n of nodes) {
      n.depth = depth;
      assignDepth(n.children, depth + 1);
    }
  };
  assignDepth(roots, 0);

  return roots;
}
