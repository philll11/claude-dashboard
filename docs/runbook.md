# Maintainer runbook

These are the steps only the maintainer can take: creating keys and storing secrets.
Each section says when it is needed, what it costs, the exact steps, and what the release workflows expect afterwards.
Use personal accounts throughout, never an employer's, so the project does not depend on a job.
Keep every key, certificate and password in a password manager, and delete local copies once they are stored as secrets.

## At a glance

| Item | Needed for | Cost |
|---|---|---|
| [Desktop update signing key](#desktop-update-signing-key) | The desktop app's releases (phase 4) | Free |

## Desktop update signing key

The desktop app only installs updates signed with this key.
If the key is lost, installed copies can never update themselves again and need installing by hand, so store it before the first release.

1. Once the desktop app exists, run `pnpm tauri signer generate -w ~/.tauri/claude-dashboard.key` from `apps/desktop` and choose a strong password.
   It writes the private key and `claude-dashboard.key.pub`.
2. Store the private key file's content and its password in your password manager.
3. Add the repository secrets `TAURI_SIGNING_PRIVATE_KEY` (the private key file's content) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
4. Put the content of `claude-dashboard.key.pub` in `plugins.updater.pubkey` in `apps/desktop/src-tauri/tauri.conf.json`.
5. Delete the local key files.
