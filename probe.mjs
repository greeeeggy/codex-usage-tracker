import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { EventEmitter } from "node:events";

class CodexUsageClient extends EventEmitter {
  constructor() {
    super();

    this.process = null;
    this.nextRequestId = 1;
    this.pendingRequests = new Map();
    this.refreshTimer = null;
  }

  async start() {
    this.process = spawn(
      "codex",
      ["app-server", "--stdio"],
      {
        windowsHide: true,
        shell: process.platform === "win32",
        stdio: ["pipe", "pipe", "pipe"],
      },
    );

    this.process.on("error", (error) => {
      this.emit("error", new Error(`Failed to start Codex: ${error.message}`));
    });

    this.process.on("exit", (code, signal) => {
      this.emit(
        "error",
        new Error(
          `Codex app-server exited. Code: ${code ?? "unknown"}, ` +
          `signal: ${signal ?? "none"}`,
        ),
      );
    });

    this.process.stderr.on("data", (data) => {
      const message = data.toString().trim();

      if (message) {
        console.error("[Codex stderr]", message);
      }
    });

    const lines = createInterface({
      input: this.process.stdout,
      crlfDelay: Infinity,
    });

    lines.on("line", (line) => {
      this.handleMessage(line);
    });

    console.log("Starting Codex app-server...");

    await this.request("initialize", {
      clientInfo: {
        name: "codex_usage_monitor",
        title: "Codex Usage Monitor",
        version: "0.1.0",
      },
    });

    console.log("Initialized successfully.");

    this.send({
      method: "initialized",
    });

    console.log("Fetching rate limits...\n");
    await this.refresh();

    this.refreshTimer = setInterval(() => {
      this.refresh().catch((error) => this.emit("error", error));
    }, 60_000);
  }

  async refresh() {
    const result = await this.request("account/rateLimits/read");

    const snapshot =
      result.rateLimitsByLimitId?.codex ??
      result.rateLimits;

    if (!snapshot) {
      throw new Error("Codex returned no rate-limit snapshot.");
    }

    this.emit("usage", normalizeSnapshot(snapshot));
  }

  request(method, params) {
    const id = this.nextRequestId++;

    const message = {
      id,
      method,
      ...(params !== undefined ? { params } : {}),
    };

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pendingRequests.delete(id);
        reject(new Error(`Request timed out: ${method}`));
      }, 15_000);

      this.pendingRequests.set(id, {
        resolve,
        reject,
        timeout,
      });

      this.send(message);
    });
  }

  send(message) {
    if (!this.process?.stdin.writable) {
      throw new Error("Codex app-server is not running.");
    }

    this.process.stdin.write(`${JSON.stringify(message)}\n`);
  }

  handleMessage(line) {
    let message;

    try {
      message = JSON.parse(line);
    } catch {
      console.error("Invalid JSON from Codex:", line);
      return;
    }

    if (message.id !== undefined) {
      const pending = this.pendingRequests.get(message.id);

      if (!pending) {
        return;
      }

      clearTimeout(pending.timeout);
      this.pendingRequests.delete(message.id);

      if (message.error) {
        pending.reject(
          new Error(
            message.error.message ??
            JSON.stringify(message.error),
          ),
        );
      } else {
        pending.resolve(message.result);
      }

      return;
    }

    if (message.method === "account/rateLimits/updated") {
      const snapshot = message.params?.rateLimits;

      if (snapshot) {
        this.emit("usage", normalizeSnapshot(snapshot));
      }
    }
  }

  stop() {
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer);
    }

    this.process?.kill();
  }
}

function normalizeSnapshot(snapshot) {
  const windows = [
    ["primary", snapshot.primary],
    ["secondary", snapshot.secondary],
  ]
    .filter(([, window]) => window)
    .map(([source, window]) => ({
      source,
      name: getWindowName(window.windowDurationMins),
      durationMinutes: window.windowDurationMins,
      usedPercent: clamp(window.usedPercent, 0, 100),
      remainingPercent: clamp(100 - window.usedPercent, 0, 100),
      resetsAt:
        window.resetsAt !== null
          ? new Date(window.resetsAt * 1000).toISOString()
          : null,
    }));

  return {
    capturedAt: new Date().toISOString(),
    limitId: snapshot.limitId,
    limitName: snapshot.limitName,
    planType: snapshot.planType,
    rateLimitReachedType: snapshot.rateLimitReachedType,
    credits: snapshot.credits,
    windows,
  };
}

function getWindowName(minutes) {
  if (minutes === 300) {
    return "fiveHour";
  }

  if (minutes === 10_080) {
    return "weekly";
  }

  if (minutes === null) {
    return "unknown";
  }

  return `${minutes}Minutes`;
}

function clamp(value, minimum, maximum) {
  return Math.min(Math.max(value, minimum), maximum);
}

const client = new CodexUsageClient();

client.on("usage", (usage) => {
  console.clear();
  console.log("=== Codex Usage Monitor (Probe) ===\n");
  console.log(JSON.stringify(usage, null, 2));
  console.log("\nListening for live updates... (Ctrl+C to quit)");
});

client.on("error", (error) => {
  console.error("\nError:", error.message);
});

process.on("SIGINT", () => {
  console.log("\nShutting down...");
  client.stop();
  process.exit(0);
});

await client.start();
