/**
 * TypeScript binding example — simple echo agent.
 *
 * Build the native addon first:
 *   cd bindings/typescript
 *   npm run build:debug
 *
 * Then run (with ts-node):
 *   npx ts-node examples/typescript-agent/example.ts
 */

import {
  AnthropicProvider,
  KaineticRuntime,
  agent,
  tool,
} from '../../bindings/typescript/index';

// ── Define a tool ──────────────────────────────────────────────────────────────

const wordCount = tool(
  { name: 'word_count', description: 'Counts words in a string.' },
  async (input: unknown) => {
    const text = String(input);
    return JSON.stringify({ count: text.split(/\s+/).length });
  },
);

// ── Define an agent ────────────────────────────────────────────────────────────

const echoAgent = agent(
  { name: 'echo', description: 'Echoes the input string.' },
  async (input: string) => `[echo] ${input}`,
);

// ── Wire up the runtime ────────────────────────────────────────────────────────

async function main(): Promise<void> {
  const apiKey = process.env['ANTHROPIC_API_KEY'] ?? 'demo-key';
  const provider = AnthropicProvider.withKey(apiKey);

  const runtime = new KaineticRuntime({
    anthropicProvider: provider,
    tools: [wordCount],
  });

  const result = await runtime.run(echoAgent, 'Hello from TypeScript!');
  console.log(result);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
