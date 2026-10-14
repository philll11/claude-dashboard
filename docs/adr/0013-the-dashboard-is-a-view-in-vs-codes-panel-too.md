# The dashboard is a view in VS Code's panel as well as a tab

In VS Code the dashboard is a webview view in a container of its own in the panel, where it opens beside the terminal, as well as the editor tab.
People keep the panel open while they work, with Claude Code running in its terminal, so the view puts what they work against in sight: a strip at its top holds the five-hour and weekly windows and the latest session in the project VS Code has open, above whichever section of the dashboard is shown.
Microsoft's guidelines ask extensions to limit custom webview views; the panel is where they put views that need width, and the dashboard needs both its charts and its tables.

The view does not keep running while hidden, as Microsoft's guidelines ask.
VS Code builds it again each time it shows it, and the web app keeps its section, its filters and the strip's figures in the view's state, so it paints them at once and brings them up to date.
The editor tab and the view are the same web app, each with a channel of its own to the server.

## Considered options

- **A tree view in the panel** would be VS Code's own kind of view, but it has no meters or charts, so the panel would hold less of the dashboard than its tab.
- **Only the strip in the panel**, with a link to the tab, would be lighter, but every look at a session or a project would leave the terminal's side.
- **Keeping the view running while hidden** would show it a moment sooner, at the cost of memory and work while nobody looks at it.
- **The view in place of the editor tab** would leave the status bar, the sidebar and wide charts without a tab to open into.

## Consequences

- The view's page is transparent, so that the panel's or the sidebar's own colour shows behind it, since no theme variable says which holds the view; its cards take their colour from the text.
  Its root keeps the colour scheme of VS Code's frame, which keeps a frame transparent only while the two match.
- With the tab and the view both open, the extension passes each answer to both, so its share of a processor while the server reads grows with the second; the integration tests measure it with both open.
- The strip asks for the latest session's whole figures at most every 10 seconds while its requests come in, since the server adds up every request of a long session to answer.
