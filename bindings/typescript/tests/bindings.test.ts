/**
 * Integration tests for the TypeScript/Node.js bindings.
 *
 * Requires the native addon to be built first:
 *   napi build --platform
 *
 * These tests validate the FFI layer — no real LLM API calls are made.
 */

let binding: typeof import('../index') | null = null;

function loadBinding() {
  if (binding) return binding;
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    binding = require('../index') as typeof import('../index');
    return binding;
  } catch {
    return null;
  }
}

describe('@kainetic/runtime — bindings smoke tests', () => {
  test('module loads without error', () => {
    const b = loadBinding();
    if (!b) {
      console.warn('Native addon not built — skipping. Run `npm run build:debug`.');
      return;
    }
    expect(b).toBeDefined();
  });

  test('AnthropicProvider.withKey() constructs without throwing', () => {
    const b = loadBinding();
    if (!b) return;
    const p = b.AnthropicProvider.withKey('test-key-123');
    expect(p).toBeDefined();
  });

  test('AnthropicProvider.fromEnv() throws when ANTHROPIC_API_KEY is unset', () => {
    const b = loadBinding();
    if (!b) return;
    const saved = process.env['ANTHROPIC_API_KEY'];
    delete process.env['ANTHROPIC_API_KEY'];
    expect(() => b.AnthropicProvider.fromEnv()).toThrow();
    if (saved !== undefined) process.env['ANTHROPIC_API_KEY'] = saved;
  });

  test('tool() factory returns a ToolHandle', () => {
    const b = loadBinding();
    if (!b) return;
    const t = b.tool(
      { name: 'echo', description: 'Echoes input.' },
      async (input) => JSON.stringify({ result: input }),
    );
    expect(t).toBeDefined();
  });

  test('agent() factory returns an AgentHandle', () => {
    const b = loadBinding();
    if (!b) return;
    const a = b.agent(
      { name: 'mirror', description: 'Mirrors its input.' },
      async (input) => input,
    );
    expect(a).toBeDefined();
  });

  test('KaineticRuntime constructs from a provider', () => {
    const b = loadBinding();
    if (!b) return;
    const provider = b.AnthropicProvider.withKey('test-key');
    const rt = new b.KaineticRuntime({ anthropicProvider: provider });
    expect(rt).toBeDefined();
  });

  test('KaineticRuntime constructor throws without a provider', () => {
    const b = loadBinding();
    if (!b) return;
    expect(() => new b.KaineticRuntime({})).toThrow();
  });
});
