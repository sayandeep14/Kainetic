import { describe, it, expect } from 'vitest';
import { buildSpanTree } from '../api/runs';
import type { Span } from '../api/types';

const makeSpan = (id: string, parent: string | null, name: string): Span => ({
  id,
  run_id: 'run-1',
  team_id: 'team-1',
  parent_span_id: parent,
  name,
  kind: 'internal',
  status: 'ok',
  start_time: '2024-01-01T00:00:00Z',
  end_time: '2024-01-01T00:00:01Z',
  attributes: {},
  events: [],
});

describe('buildSpanTree', () => {
  it('returns empty array for empty input', () => {
    expect(buildSpanTree([])).toEqual([]);
  });

  it('treats spans with unknown parent as roots', () => {
    const spans: Span[] = [makeSpan('a', 'missing-parent', 'child')];
    const tree = buildSpanTree(spans);
    expect(tree).toHaveLength(1);
    expect(tree[0].id).toBe('a');
    expect(tree[0].depth).toBe(0);
  });

  it('builds single root with children', () => {
    const spans: Span[] = [
      makeSpan('root', null, 'root'),
      makeSpan('child1', 'root', 'child1'),
      makeSpan('child2', 'root', 'child2'),
    ];
    const tree = buildSpanTree(spans);
    expect(tree).toHaveLength(1);
    expect(tree[0].children).toHaveLength(2);
  });

  it('assigns depth correctly', () => {
    const spans: Span[] = [
      makeSpan('r', null, 'r'),
      makeSpan('c', 'r', 'c'),
      makeSpan('g', 'c', 'g'),
    ];
    const tree = buildSpanTree(spans);
    const root = tree[0];
    const child = root.children[0];
    const grandchild = child.children[0];
    expect(root.depth).toBe(0);
    expect(child.depth).toBe(1);
    expect(grandchild.depth).toBe(2);
  });

  it('computes durationMs from timestamps', () => {
    const spans: Span[] = [makeSpan('s', null, 's')];
    spans[0].start_time = '2024-01-01T00:00:00.000Z';
    spans[0].end_time = '2024-01-01T00:00:01.500Z';
    const tree = buildSpanTree(spans);
    expect(tree[0].durationMs).toBe(1500);
  });

  it('sets durationMs null when end_time is missing', () => {
    const spans: Span[] = [makeSpan('s', null, 's')];
    spans[0].end_time = null;
    const tree = buildSpanTree(spans);
    expect(tree[0].durationMs).toBeNull();
  });
});
