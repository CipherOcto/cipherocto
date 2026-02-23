/**
 * CipherOcto Agent Example
 *
 * This is a simple "hello world" agent that demonstrates
 * how agents work in the CipherOcto ecosystem.
 *
 * Agent execution flow:
 * 1. CipherOcto runtime spawns this agent
 * 2. Runtime provides execution context (tasks, permissions)
 * 3. Agent processes the task and returns results
 * 4. Runtime persists results and handles payment
 */

interface OctoContext {
  // Task provided by the runtime
  task: {
    type: string;
    input: unknown;
  };

  // Permissions granted by manifest
  permissions: string[];

  // Agent identity
  agentId: string;

  // Logging function
  log: (message: string) => void;
}

/**
 * Main agent execution function
 *
 * Called by CipherOcto runtime when a task is assigned
 */
export async function run(ctx: OctoContext): Promise<unknown> {
  ctx.log(`🤖 Hello from ${ctx.agentId}!`);
  ctx.log(`📋 Processing task: ${ctx.task.type}`);
  ctx.log(`🔐 Permissions: ${ctx.permissions.join(", ")}`);

  // Process the task based on its type
  switch (ctx.task.type) {
    case "greeting":
      return handleGreeting(ctx);
    case "analysis":
      return handleAnalysis(ctx);
    default:
      return {
        success: false,
        error: `Unknown task type: ${ctx.task.type}`
      };
  }
}

/**
 * Handle simple greeting tasks
 */
async function handleGreeting(ctx: OctoContext): Promise<object> {
  const input = ctx.task.input as { name?: string };
  const name = input?.name || "World";

  ctx.log(`👋 Greeting: Hello, ${name}!`);

  return {
    success: true,
    message: `Hello, ${name}! This is ${ctx.agentId} running on CipherOcto.`,
    timestamp: new Date().toISOString()
  };
}

/**
 * Handle analysis tasks (demonstrates external service calls)
 */
async function handleAnalysis(ctx: OctoContext): Promise<object> {
  ctx.log(`🔍 Analyzing input...`);

  // Example: Agent could hire other specialized agents here
  // For now, we return a simple analysis

  const input = ctx.task.input as { text?: string };
  const text = input?.text || "";

  return {
    success: true,
    analysis: {
      length: text.length,
      wordCount: text.split(/\s+/).length,
      hasNumbers: /\d/.test(text),
      processedAt: new Date().toISOString()
    }
  };
}

// For direct testing (optional)
if (import.meta.main) {
  const mockContext: OctoContext = {
    task: { type: "greeting", input: { name: "CipherOcto" } },
    permissions: ["net", "read"],
    agentId: "hello-agent",
    log: console.log
  };

  run(mockContext).then(result => {
    console.log("Result:", result);
  });
}
