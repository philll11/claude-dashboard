# Charts in VS Code use our validated model colours, not VS Code's chart colours

Inside VS Code the dashboard takes every colour from the person's theme through its `--vscode-*` variables, except the colours of the model series in charts.
Those come from our own palette, in its dark or light set to match the theme kind, because VS Code's `--vscode-charts-*` colours fail the checks a categorical palette must pass: in Dark Modern they fail the lightness-band and normal-vision checks, and in Light Modern they fail the colour-blind separation check.
High Contrast themes keep the same colours and add textures, so that no series depends on colour alone.

## Considered options

- **VS Code's chart colours** match the theme exactly, but models that look alike would be told apart by their labels alone.
- **Our whole palette instead of the theme** would keep charts and chrome consistent across surfaces, but the panel would stop looking like part of VS Code, which Microsoft's webview guidelines ask extensions to avoid.
