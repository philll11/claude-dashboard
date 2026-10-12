# The desktop app outlives its window

The desktop app has an item in the notification area, or the menu bar on macOS, that shows the five-hour window at a glance.
While the item is there and the person keeps the app running, closing the window hides it, and the app runs on until the person chooses Quit in the item's menu.
The item, a second start of the app and, on macOS, the Dock bring the window back.

Keeping the app running is how closing the window works on Windows and macOS until the person changes it in Settings.
On Linux, closing the window quits unless the person switches keeping it running on, because GNOME shows no notification area without an extension, and an app with nothing on screen would be lost.
The first time the window closes on Windows, the dashboard asks which the person wants, and remembers the answer.
Opening at login, which the person switches on in Settings, starts the app in its item without the window.

The server still lives and dies with the app (ADR 0008); hiding the window changes nothing for it.

## Considered options

- **Quitting when the window closes**, with the item only while the window is open.
  The item is for a glance at the plan limits while the dashboard is not open, which this would rule out.
- **A small process of its own for the item**, apart from the app.
  It would need its own connection to the history, its own updates and a way to start the app, and the person would have two programs to keep running.

## Consequences

- The hidden window keeps its web view, so it reopens at once and the app still checks for updates every six hours.
  An update that arrives while the window is hidden downloads and waits in the item's menu, rather than restarting an app the person cannot see.
- Tray calls run on the main thread, so the item reads the history and draws its icon on a thread of its own, which never waits on the main thread and stops before the server.
- Where a desktop has no tray library, the app runs without the item and quits when its window closes.
- The end-to-end tests switch keeping the app running off, because they end the app by closing its window.
