// Writes a synthetic Claude Code folder on a thread of its own, for `prepare`, so that the process
// that asked for it, such as the test bridge's server, stays free to answer meanwhile.
//
// Node loads this file as it is, so it keeps to TypeScript that only needs its types removed.

import { parentPort, workerData } from "node:worker_threads";

import { type Options, writeClaudeFolder } from "./synthetic.ts";

const { root, options } = workerData as { root: string; options: Options | undefined };
parentPort?.postMessage(writeClaudeFolder(root, options));
