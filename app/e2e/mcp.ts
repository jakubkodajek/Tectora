// A minimal MCP client over stdio for the `tectora mcp` server: what an AI
// client (Claude Desktop, Codex, …) does, so the tests can check that every
// AI edit reaches the open app and matches the same edit made in the UI.
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createInterface } from "node:readline";
import { CONFIG_DIR, LIBRARY_DIR, TECTORA_BIN } from "./env";

export class McpError extends Error {}

export class Mcp {
  private proc: ChildProcessWithoutNullStreams;
  private next = 1;
  private waiting = new Map<number, (msg: Record<string, unknown>) => void>();
  readonly stderr: string[] = [];

  private constructor(readonly client: string) {
    const env: NodeJS.ProcessEnv = { ...process.env, TECTORA_CONFIG_DIR: CONFIG_DIR };
    delete env.TECTORA_PROJECTS_DIR;
    this.proc = spawn(TECTORA_BIN, ["mcp", "--dir", LIBRARY_DIR], { env });
    createInterface({ input: this.proc.stdout }).on("line", (line) => {
      if (!line.trim()) return;
      const msg = JSON.parse(line) as Record<string, unknown>;
      const cb = this.waiting.get(msg.id as number);
      if (cb) {
        this.waiting.delete(msg.id as number);
        cb(msg);
      }
    });
    this.proc.stderr.on("data", (d) => this.stderr.push(String(d)));
  }

  /** Starts the server and runs the MCP handshake as client `client`. */
  static async start(client = "Claude"): Promise<Mcp> {
    const m = new Mcp(client);
    await m.request("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: client, version: "1.0" },
    });
    m.proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
    return m;
  }

  request(method: string, params: unknown): Promise<Record<string, unknown>> {
    const id = this.next++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new McpError(`MCP ${method} timed out\n${this.stderr.join("")}`)), 20_000);
      this.waiting.set(id, (msg) => {
        clearTimeout(timer);
        if (msg.error) reject(new McpError(JSON.stringify(msg.error)));
        else resolve(msg.result as Record<string, unknown>);
      });
      this.proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  }

  /** Calls a tool; returns its text parsed as JSON when it is JSON. Throws on a tool error. */
  async call<T = any>(name: string, args: Record<string, unknown> = {}): Promise<T> {
    const r = await this.tryCall(name, args);
    if (r.isError) throw new McpError(`${name}: ${r.text}`);
    return r.value as T;
  }

  /** Calls a tool and returns the error flag and text instead of throwing. */
  async tryCall(name: string, args: Record<string, unknown> = {}): Promise<{ isError: boolean; text: string; value: unknown }> {
    const res = await this.request("tools/call", { name, arguments: args });
    const text = ((res.content as { text?: string }[] | undefined) ?? []).map((c) => c.text ?? "").join("\n");
    let value: unknown = text;
    try {
      value = JSON.parse(text);
    } catch {
      // Plain text answer.
    }
    return { isError: !!res.isError, text, value };
  }

  async tools(): Promise<string[]> {
    const r = await this.request("tools/list", {});
    return (r.tools as { name: string }[]).map((t) => t.name);
  }

  stop() {
    this.proc.kill();
  }
}
