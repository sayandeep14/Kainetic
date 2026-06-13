import { describe, it, expect } from 'vitest';

/** Inline copy of the percentile helper to keep the test self-contained. */
function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, idx)];
}

describe('percentile', () => {
  it('returns 0 for empty array', () => {
    expect(percentile([], 50)).toBe(0);
  });

  it('returns the only element for a single-element array', () => {
    expect(percentile([100], 50)).toBe(100);
    expect(percentile([100], 99)).toBe(100);
  });

  it('computes p50 correctly', () => {
    const sorted = [10, 20, 30, 40, 50];
    expect(percentile(sorted, 50)).toBe(30);
  });

  it('computes p99 on larger array', () => {
    const sorted = Array.from({ length: 100 }, (_, i) => i + 1);
    expect(percentile(sorted, 99)).toBe(99);
  });

  it('p100 returns last element', () => {
    const sorted = [1, 2, 3, 4, 5];
    expect(percentile(sorted, 100)).toBe(5);
  });
});
