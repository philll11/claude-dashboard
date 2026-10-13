# A release goes public only when it is complete

A release's GitHub Release starts as a draft, with the title and notes dist plans from the changelog.
The VS Code extension's packages and the desktop app's installers, update bundles and `latest.json` are built from the programs of the same run and attached to the draft.
dist's host job then attaches the programs and publishes the draft, which it does only when every job before it has succeeded (`create-release = false`).
A release that fails part-way stays a draft, which only the maintainer sees.

Installed desktop apps look for updates at `releases/latest/download/latest.json`, so the latest release always holds the manifest and every file it names.
Once a release is published, the install check runs its programs and the release check runs the end-to-end tests on its own installers and packages, verifying every file's attestation.

## Considered options

- **Publishing first and attaching the desktop and VS Code files afterwards**, as dist's post-announce jobs do.
  The release is then the latest for about 20 minutes without them, and a desktop app that checks for updates in that time finds no `latest.json` and waits for its next check.
- **Bundling the extra files into dist's own upload**, as workflow artifacts named `artifacts-*`.
  dist's template uploads them, but its documentation does not promise it, so an upgrade of dist could leave the files out unnoticed.
- **A fixed update manifest outside the releases**, such as a release that never changes its tag.
  It closes the gap too, but leaves a release in the list that is not a release, and installed versions would still read the old address.

## Consequences

- The desktop and VS Code builds take the programs from the run that builds them and check them against their checksums; dist attests the programs when it publishes, since pull requests from Dependabot cannot write attestations, and the release check verifies every attestation afterwards.
- A release that stays a draft is finished by running the failed jobs again, or abandoned by deleting the draft and its tag, as the runbook says.
- Pull requests build the programs as a release would, while CI's own Desktop and VS Code jobs build and test the app and the extension.
