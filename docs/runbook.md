# Maintainer runbook

These are the steps only the maintainer can take: creating keys and storing secrets.
Each section says when it is needed, what it costs, the exact steps, and what the release workflows expect afterwards.
Use personal accounts throughout, never an employer's, so the project does not depend on a job.
Keep every key, certificate and password in a password manager, and delete local copies once they are stored as secrets.

## At a glance

| Item | Needed for | Cost |
|---|---|---|
| [Desktop update signing key](#desktop-update-signing-key) | Every desktop release | Free |

## Desktop update signing key

The desktop app installs only updates signed with this key, which is separate from code signing: Tauri's updater always checks it.
The repository secrets `TAURI_SIGNING_PRIVATE_KEY` (the private key file's content) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` hold it for the desktop release workflow, and its public key is `plugins.updater.pubkey` in `apps/desktop/src-tauri/tauri.conf.json`.
Keep a copy of the private key and its password in your password manager.

To replace the key, as when it is lost or has leaked:

1. From `apps/desktop`, run `pnpm exec tauri signer generate -w <a folder of your own>/claude-dashboard.key` and choose a strong password.
   It writes the private key and `claude-dashboard.key.pub`.
2. Set the two repository secrets to the new private key file's content and its password, for example with `gh secret set TAURI_SIGNING_PRIVATE_KEY < claude-dashboard.key`.
3. Put the content of `claude-dashboard.key.pub` in `plugins.updater.pubkey`, and release a new version.
4. Store the private key and its password in your password manager, and delete the local files.

Installed copies only trust the key they were built with, so each one needs the new version installed by hand once.
