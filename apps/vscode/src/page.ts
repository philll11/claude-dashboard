// The page the dashboard's web app runs in, in the editor tab and in the view: it runs only its own
// scripts and loads only the built web app, which lays itself out for where the page says it is.

import * as crypto from "node:crypto";

import * as vscode from "vscode";

/** Where the page is: an editor tab, or the view in the panel or a sidebar. */
export type Placement = "tab" | "view";

/** What the tab and the view are called. */
export const TITLE = "Usage dashboard";

/** The folder the built web app is in, the only one the page may load from. */
function webviewRoot(extensionUri: vscode.Uri): vscode.Uri {
  return vscode.Uri.joinPath(extensionUri, "dist", "webview");
}

export function pageOptions(extensionUri: vscode.Uri): vscode.WebviewOptions {
  return { enableScripts: true, localResourceRoots: [webviewRoot(extensionUri)] };
}

export function page(
  webview: vscode.Webview,
  extensionUri: vscode.Uri,
  placement: Placement,
): string {
  const nonce = crypto.randomBytes(16).toString("base64");
  const asset = (name: string) =>
    webview.asWebviewUri(vscode.Uri.joinPath(webviewRoot(extensionUri), name)).toString();
  const policy = [
    "default-src 'none'",
    `style-src ${webview.cspSource}`,
    `img-src ${webview.cspSource} data:`,
    `font-src ${webview.cspSource}`,
    `script-src 'nonce-${nonce}'`,
  ].join("; ");
  return `<!doctype html>
<html lang="en-GB">
  <head>
    <meta charset="utf-8" />
    <meta http-equiv="Content-Security-Policy" content="${policy}" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${TITLE}</title>
    <link rel="stylesheet" href="${asset("dashboard.css")}" />
  </head>
  <body data-placement="${placement}">
    <div id="root"></div>
    <script type="module" nonce="${nonce}" src="${asset("dashboard.js")}"></script>
  </body>
</html>`;
}
