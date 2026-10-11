// The messages between the dashboard and `claude-dashboard serve`. The types come from the Rust
// server (src/generated/protocol.ts, written by ts-rs); this file says which request takes and
// returns which, and how a surface carries them. webview.ts holds the messages a VS Code webview
// and its extension exchange.

import type {
  Choices,
  EndpointParams,
  FolderParams,
  ImportDone,
  ImportOffer,
  Health,
  HistoryChanged,
  Limits,
  Overview,
  OverviewParams,
  Prices,
  PricesParams,
  Progress,
  SessionDetail,
  SessionParams,
  SessionsPage,
  SessionsParams,
  Settings,
  SettingsChange,
  Status,
  StatusLineApply,
  StatusLineParams,
  StatusLinePlan,
} from "./generated/protocol";

export type * from "./generated/protocol";
export type * from "./webview";

/** Each request the server answers, as its parameters and its result. */
export interface Requests {
  status: [null, Status];
  choices: [null, Choices];
  overview: [OverviewParams, Overview];
  sessions: [SessionsParams, SessionsPage];
  session: [SessionParams, SessionDetail];
  limits: [null, Limits];
  prices: [PricesParams, Prices];
  health: [null, Health];
  refresh: [null, null];
  "endpoint/set": [EndpointParams, null];
  "statusLine/plan": [StatusLineParams, StatusLinePlan];
  "statusLine/apply": [StatusLineApply, null];
  settings: [null, Settings];
  "settings/set": [SettingsChange, null];
  "folders/add": [FolderParams, null];
  "folders/remove": [FolderParams, null];
  "import/offer": [null, ImportOffer | null];
  "import/run": [null, ImportDone];
  "import/dismiss": [null, null];
}

/** Each notification the server sends, and what it carries. */
export interface Notifications {
  "ingest/progress": Progress;
  "history/changed": HistoryChanged;
  "limits/changed": null;
}

export type Method = keyof Requests;
export type Params<M extends Method> = Requests[M][0];
export type Result<M extends Method> = Requests[M][1];

/** How the dashboard reaches the server, in VS Code or in the desktop app. */
export interface Backend {
  request<M extends Method>(method: M, params: Params<M>): Promise<Result<M>>;
  /** Calls `listener` for each notification `method` until the returned function is called. */
  onNotification<N extends keyof Notifications>(
    method: N,
    listener: (params: Notifications[N]) => void,
  ): () => void;
}
