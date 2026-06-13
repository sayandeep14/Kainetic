/* eslint-disable */
/* tslint:disable */
/* auto-generated TypeScript declarations for @kainetic/runtime */

/** Anthropic Claude provider. */
export declare class AnthropicProvider {
  /** Create from the ANTHROPIC_API_KEY environment variable. */
  static fromEnv(): AnthropicProvider;
  /** Create with an explicit API key. */
  static withKey(apiKey: string): AnthropicProvider;
}

/** OpenAI provider. */
export declare class OpenAiProvider {
  /** Create from the OPENAI_API_KEY environment variable. */
  static fromEnv(): OpenAiProvider;
  /** Create with an explicit API key. */
  static withKey(apiKey: string): OpenAiProvider;
}

/** Options for the `tool()` factory. */
export interface ToolOptions {
  /** Short snake-case name shown to the model. */
  name: string;
  /** One-sentence description shown to the model. */
  description: string;
}

/** An opaque handle to a registered Kainetic tool. */
export declare class ToolHandle {}

/**
 * Register a JavaScript async function as a Kainetic tool.
 *
 * @param options  - Tool metadata.
 * @param fn       - Async function that receives JSON input and returns a JSON string.
 *
 * @example
 * const greet = tool(
 *   { name: 'greet', description: 'Returns a greeting.' },
 *   async (input) => JSON.stringify({ greeting: `Hello, ${input.name}!` })
 * );
 */
export declare function tool(options: ToolOptions, fn: (input: unknown) => Promise<string>): ToolHandle;

/** Options for the `agent()` factory. */
export interface AgentOptions {
  /** Stable agent identifier. */
  name: string;
  /** One-sentence description. */
  description: string;
}

/** An opaque handle to a registered Kainetic agent. */
export declare class AgentHandle {}

/**
 * Register a JavaScript async function as a Kainetic agent.
 *
 * @param options  - Agent metadata.
 * @param fn       - Async function `(input: string) => Promise<string>`.
 *
 * @example
 * const echo = agent(
 *   { name: 'echo', description: 'Echoes input.' },
 *   async (input) => input
 * );
 */
export declare function agent(options: AgentOptions, fn: (input: string) => Promise<string>): AgentHandle;

/** Options for `KaineticRuntime`. */
export interface RuntimeOptions {
  /** Anthropic Claude provider. */
  anthropicProvider?: AnthropicProvider;
  /** OpenAI provider. */
  openaiProvider?: OpenAiProvider;
  /** Optional default tools for every agent. */
  tools?: ToolHandle[];
}

/**
 * The top-level Kainetic async runtime for Node.js.
 *
 * @example
 * const runtime = new KaineticRuntime({ anthropicProvider: AnthropicProvider.fromEnv() });
 * const result = await runtime.run(myAgent, 'hello');
 */
export declare class KaineticRuntime {
  constructor(options: RuntimeOptions);
  /**
   * Run an agent and resolve with its string output.
   *
   * @param agent - Agent created by `agent()`.
   * @param input - String input for the agent.
   */
  run(agent: AgentHandle, input: string): Promise<string>;
}
