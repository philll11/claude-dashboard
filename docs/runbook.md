# Maintainer runbook

These are the steps only the maintainer can take: creating accounts, granting access and storing secrets.
Each section says when it is needed, what it costs, the exact steps, and what the release workflows expect afterwards.
Use personal accounts throughout, never an employer's, so the project does not depend on a job.
Keep every key, certificate and password in a password manager, and delete local copies once they are stored as secrets.

## At a glance

| Item | Needed for | Cost |
|---|---|---|
| [Visual Studio Marketplace](#visual-studio-marketplace) | First VS Code pre-release (phase 3) | Free; needs an Azure subscription, which a free account provides |
| [Open VSX](#open-vsx) | First VS Code pre-release (phase 3) | Free |
| [Desktop update signing key](#desktop-update-signing-key) | First desktop pre-release (phase 4) | Free |
| [Apple Developer Program](#apple-developer-program) | Signed macOS desktop app (phase 4) | US$99 a year |
| [Windows signing with SignPath Foundation](#windows-signing-with-signpath-foundation) | After the first public release | Free for open source |
| [Homebrew tap](#homebrew-tap) | Package managers (phase 7) | Free |
| [winget](#winget) | Package managers (phase 7) | Free |
| [Scoop bucket](#scoop-bucket) | Package managers (phase 7) | Free |

## Identifiers

- Marketplace publisher ID and Open VSX namespace: `philll11`, in lower case, because Open VSX trusted publishing compares the case of the `publisher` field.
- Extension ID: `philll11.claude-dashboard`.

## Visual Studio Marketplace

Azure DevOps personal access tokens stop working on 1 December 2026, so publishing uses a Microsoft Entra managed identity that trusts this repository's GitHub Actions.
Microsoft documents managed-identity publishing for Azure Pipelines; the GitHub Actions steps below use the same identity with GitHub as the trusted issuer.

1. Go to <https://marketplace.visualstudio.com/manage>, sign in with your personal Microsoft account and select **Create publisher**.
   Set the ID to `philll11` (it can never be changed) and the name to your name, accept the publisher agreement and select **Create**.
2. Sign in to <https://portal.azure.com> with the same personal account.
   Without a subscription, create a free account: Azure asks for a card to verify your identity and charges nothing unless you move to pay-as-you-go.
3. Create a resource group `claude-dashboard-publishing`, and in it a **user-assigned managed identity** called `claude-dashboard-marketplace`.
   Record its **Client ID** and **Tenant ID** from the identity's overview page.
4. In the identity, open **Federated credentials**, select **Add credential** and choose **GitHub Actions deploying Azure resources**.
   Organisation `philll11`, repository `claude-dashboard`, entity **Environment**, environment `vscode-marketplace`, name `github-vscode-marketplace`.
5. In this repository, open **Settings**, **Environments**, **New environment**, and create `vscode-marketplace`.
   Add the environment variables (not secrets; they identify the identity but grant nothing) `AZURE_CLIENT_ID` (the Client ID) and `AZURE_TENANT_ID` (the Tenant ID).
   Set **Deployment branches and tags** to **Selected branches and tags** with the tag pattern `v*`, so only release tags can publish.
6. The Marketplace knows the identity by an id of its own.
   The **Visual Studio Marketplace** job of the first release's **VS Code release** workflow prints it in its step **Show the identity that publishes**, and then fails to publish, because the identity is not a member of the publisher yet.
7. On your publisher page, open **Members**, select **Add**, paste that id and give it the **Contributor** role, which can publish but cannot change publisher settings or access.
   Then re-run the failed job.

The workflow signs in with `azure/login` (OpenID Connect, `allow-no-subscriptions: true`) and runs `vsce publish --azure-credential`.

## Open VSX

Cursor, VSCodium and Windsurf (Devin Desktop) install extensions from Open VSX.
Claim the namespace before the first publish: Open VSX blocks publishing into a namespace that also exists on the Visual Studio Marketplace unless the namespace is verified.

1. Create an Eclipse account at <https://accounts.eclipse.org/user/register>, and fill in the **GitHub Username** field with `philll11`.
2. At <https://open-vsx.org>, sign in with GitHub as `philll11`.
   Open your avatar, **Settings**, **Profile**, select **Log in with Eclipse**, then **Show Publisher Agreement** and **Agree**.
3. In **Settings**, **Access Tokens**, select **Generate New Token** and copy it; it is shown only once.
4. Create the namespace: `npx ovsx create-namespace philll11 -p <token>`.
5. Open an issue in <https://github.com/EclipseFdn/open-vsx.org/issues/new/choose> with the **Claim namespace ownership** template for `philll11`.
   It asks for a GitHub account with at least 12 months of public history, and for evidence that the extension's repository belongs to that account.
6. In this repository, create the environment `open-vsx`, add the secret `OVSX_PAT` with the token, and limit its **Deployment branches and tags** to the tag pattern `v*`, as for the Marketplace.
   The first release publishes with it.
7. After the first version is on Open VSX, switch to trusted publishing at <https://open-vsx.org/user-settings/trusted-publishers>.
   Choose the `philll11` namespace and the provider **GitHub Actions**, with owner `philll11`, repository `claude-dashboard`, workflow `release.yml` and environment `open-vsx`.
   A release tag starts `release.yml`, which runs the publishing job from `vscode-release.yml`; if Open VSX refuses the token and names `vscode-release.yml`, register that workflow instead.
8. Delete the `OVSX_PAT` secret and revoke the token on Open VSX.
   Without the secret, the **Open VSX** job publishes through trusted publishing, exchanging GitHub's OpenID Connect token for a short-lived one.

## Desktop update signing key

The desktop app only installs updates signed with this key.
If the key is lost, installed copies can never update again, so store it before the first release.

1. Once the desktop app exists, run `pnpm tauri signer generate -w ~/.tauri/claude-dashboard.key` from `apps/desktop` and choose a strong password.
   It writes the private key and `claude-dashboard.key.pub`.
2. Store the private key file's content and its password in your password manager.
3. Add the repository secrets `TAURI_SIGNING_PRIVATE_KEY` (the private key file's content) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
4. Send me the content of `claude-dashboard.key.pub`; it goes into `tauri.conf.json`.
5. Delete the local key files.

## Apple Developer Program

Without Developer ID signing and notarisation, macOS blocks the desktop app when it is downloaded in a browser.
The command-line tool installed with Homebrew or the shell installer does not need it.

1. Enrol as an individual at <https://developer.apple.com/programs/enroll/> (US$99 a year).
2. Create a certificate signing request on any computer with OpenSSL:
   `openssl req -new -newkey rsa:2048 -nodes -keyout developer-id.key -out developer-id.csr -subj "/emailAddress=<your Apple ID email>/CN=<your name>"`
3. At <https://developer.apple.com/account/resources/certificates/list>, add a certificate, choose **Developer ID Application**, upload `developer-id.csr` and download the `.cer` file.
4. Combine the certificate and key into a password-protected `.p12`, and encode it:
   `openssl x509 -inform DER -in developerID_application.cer -out developer-id.pem`
   `openssl pkcs12 -export -legacy -inkey developer-id.key -in developer-id.pem -out developer-id.p12`
   `openssl base64 -A -in developer-id.p12 -out developer-id.p12.base64`
5. For notarisation, create an App Store Connect API key: <https://appstoreconnect.apple.com>, **Users and Access**, **Integrations**, **Team Keys**, add a key with **Developer** access.
   Note the **Issuer ID** above the table and the **Key ID**, and download the `.p8` file, which can be downloaded only once.
6. Add the repository secrets:
   - `APPLE_CERTIFICATE`: the content of `developer-id.p12.base64`.
   - `APPLE_CERTIFICATE_PASSWORD`: the `.p12` export password.
   - `APPLE_API_ISSUER`: the Issuer ID.
   - `APPLE_API_KEY`: the Key ID.
   - `APPLE_API_PRIVATE_KEY`: the content of the `.p8` file; the workflow writes it to a file and points `APPLE_API_KEY_PATH` at it.
7. Store the originals in your password manager and delete the local files.

## Windows signing with SignPath Foundation

SignPath Foundation signs open-source projects for free.
It accepts a project only once it has been released in the form to be signed, so apply after the first public release.
Signed files show "SignPath Foundation" as the publisher, and each release needs your approval in SignPath before it is signed.

1. Read the conditions at <https://signpath.org/terms>.
   Everyone with write access to the repository must use multi-factor authentication.
2. Tell me before applying: the README needs a code signing policy that includes "Free code signing provided by SignPath.io, certificate by SignPath Foundation".
3. Apply at <https://signpath.org/apply>.
4. After approval, in the SignPath app: add the trusted build system **GitHub.com** to the organisation and link it to the project, and install <https://github.com/apps/signpath> on this repository.
5. Create a CI user in SignPath and generate its API token, which is shown only once.
6. Add the repository secret `SIGNPATH_API_TOKEN`, and the repository variables `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG` and `SIGNPATH_SIGNING_POLICY_SLUG`, all shown on the signing policy's page.

## Homebrew tap

1. Create the public repository `philll11/homebrew-tap` with a README, or ask me to create it.
2. Create a fine-grained personal access token at <https://github.com/settings/personal-access-tokens/new> with access to `philll11/homebrew-tap` only and the permission **Contents: Read and write**.
3. Add it to this repository as the secret `HOMEBREW_TAP_TOKEN`.

If the release's Homebrew job reports a permission error, replace the token with a classic token with the `repo` scope, which is what dist's documentation describes.

## winget

1. Create a classic personal access token at <https://github.com/settings/tokens/new> with the `public_repo` scope; winget's tooling does not support fine-grained tokens.
2. Add it to this repository as the secret `WINGET_TOKEN`.
3. The first submission to `microsoft/winget-pkgs` opens a pull request from your account.
   When Microsoft's CLA bot comments on it, reply as the bot asks to accept the agreement; it covers all Microsoft repositories from then on.
4. A moderator reviews the first submission; later versions are submitted by the release workflow.

## Scoop bucket

1. Create the public repository `philll11/scoop-bucket` from the template <https://github.com/ScoopInstaller/BucketTemplate>, or ask me to create it.
2. Add the topic `scoop-bucket` so Scoop's search finds it.

The template's scheduled workflow updates the manifest after each release, so no token is needed.
