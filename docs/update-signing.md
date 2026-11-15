# Update signing and channels

PSG PDF and PSG Academic PDF update themselves from this repository's GitHub releases. Each
release is signed with PSG's own Ed25519 key, so updates need no paid code-signing certificate.

## How a release is checked

1. The release workflow (`packaging/ci/release-files.sh`) writes `SHA256SUMS.txt` over every file,
   signs its exact bytes with the `UPDATE_SIGNING_KEY` secret and publishes the signature as
   `SHA256SUMS.txt.sig`. It checks the signature against the public key the apps embed and refuses
   to publish when the secret is missing or doesn't match.
2. The app picks the one file for its own product, platform and install type
   (`psg-pdf-0.20.0-windows-x64.msi`, …), fetches the list and the signature, and checks the
   signature with the public key in [`packaging/update-signing-key.txt`](../packaging/update-signing-key.txt).
   A list that isn't signed by that key stops the update before anything is downloaded.
3. It downloads the file and checks its SHA-256 against the signed list, then checks it once more
   just before installing, so a file changed in the download folder is never installed.
4. It installs and restarts.

The private key exists only in the repository secret. Nobody can read a secret back, not even the
repository's owner; the workflow is the only place that uses it. With the public key file empty,
the apps keep craft-update's original checks (an Authenticode or Developer ID signature).

## Setting it up (once)

On a computer with OpenSSL (on Windows it comes with Git for Windows), in PowerShell:

```powershell
$openssl = "$env:ProgramFiles\Git\usr\bin\openssl.exe"
& $openssl genpkey -algorithm ed25519 -out $HOME\psg-update-key.pem
& $openssl pkey -in $HOME\psg-update-key.pem -pubout -outform DER -out $HOME\psg-update-key.pub.der
-join ((Get-Content $HOME\psg-update-key.pub.der -Encoding Byte)[-32..-1] | ForEach-Object { $_.ToString('x2') })
Get-Content $HOME\psg-update-key.pem -Raw | Set-Clipboard
```

On macOS or Linux:

```sh
openssl genpkey -algorithm ed25519 -out ~/psg-update-key.pem
openssl pkey -in ~/psg-update-key.pem -pubout -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n'; echo
```

1. Add the contents of `psg-update-key.pem` (on Windows now on the clipboard) as the repository
   secret `UPDATE_SIGNING_KEY` (*Settings ▸ Secrets and variables ▸ Actions*).
2. Put the 64 hex digits printed above into `packaging/update-signing-key.txt` and merge that.
3. Keep `psg-update-key.pem` somewhere safe and private. Losing it means a new key, and copies
   that embed the old public key can't update to releases signed with the new one: they need one
   manual install.

Copies built before the key was set up can't update to signed releases on Windows (they still
expect an Authenticode signature): install the first signed release by hand, once.

## Channels

- **Stable** (default): only stable releases are offered.
- **Beta**: the newest release is offered, beta or stable. A copy on `0.20.0-beta.3` updates to
  `0.20.0-beta.4`, then to `0.20.0` when it ships, then to `0.21.0-beta.1`.

PSG PDF: *Preferences ▸ General ▸ Get beta updates* (`beta_updates`). PSG Academic PDF: *Get beta
updates* under *Check for updates*. Joining and leaving each ask for a second confirmation.
Leaving never installs an older version: the copy stays on its beta until a newer stable release
ships.
