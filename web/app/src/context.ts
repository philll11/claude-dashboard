// What every part of the dashboard can reach: the surface it runs in, and the server's status.

import { createContext, useContext } from "react";

import type { Status } from "@claude-dashboard/protocol";

import type { Host } from "./host";

export const HostContext = createContext<Host | null>(null);

export function useHost(): Host {
  const host = useContext(HostContext);
  if (!host) throw new Error("useHost needs a HostContext");
  return host;
}

export const StatusContext = createContext<Status | null>(null);

/** The server's status, which sections only render with. */
export function useStatus(): Status {
  const status = useContext(StatusContext);
  if (!status) throw new Error("useStatus needs a StatusContext");
  return status;
}
