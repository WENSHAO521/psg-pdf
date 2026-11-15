# Free Windows signing with a self-signed certificate

The in-app updater on Windows installs an update only when **the running copy and the download
both carry a valid Authenticode signature from the same publisher** (craft-update,
`self_update::signed_like`). An unsigned build therefore says *"this copy of PSG PDF isn't signed
… so it doesn't install updates"*.

A certificate from a public authority costs money. A self-signed certificate is free and satisfies
the updater on every computer that trusts it. What it does not do: remove the SmartScreen warning on
a first download, or make Windows trust the files on computers that haven't trusted it.

## 1. Make the certificate (once)

On a Windows machine, from a checkout:

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\new-signing-cert.ps1 -OutDir $HOME\psg-signing
```

It writes `psg-codesign.pfx` (private, back it up), `psg-codesign.pfx.base64`,
`psg-codesign-password.txt` and `psg-codesign.cer` (public). Keep the first three out of the
repository. Losing the `.pfx` means a new certificate that every computer has to trust again.

## 2. Add the repository secrets

*Settings ▸ Secrets and variables ▸ Actions ▸ New repository secret*:

| Secret | Value |
|---|---|
| `WINDOWS_CERTIFICATE` | the contents of `psg-codesign.pfx.base64` |
| `WINDOWS_CERTIFICATE_PASSWORD` | the contents of `psg-codesign-password.txt` |

The next push to `beta` or `main` signs both products' programs and MSIs. `sign.ps1` sees that
the certificate is self-signed and trusts it on the CI runner only, so `signtool verify` and
`packaging/ci/verify-signatures.sh` still check every signature.

## 3. Trust the certificate on each computer (once per computer)

In an administrator PowerShell, for every account on that computer:

```powershell
Import-Certificate -FilePath psg-codesign.cer -CertStoreLocation Cert:\LocalMachine\Root
```

Without administrator rights, for the current account only (Windows asks to confirm):

```powershell
Import-Certificate -FilePath psg-codesign.cer -CertStoreLocation Cert:\CurrentUser\Root
```

Trust only a `.cer` you made yourself or received from its owner: a trusted certificate can sign
programs that Windows then accepts. To undo, delete it from *certmgr.msc ▸ Trusted Root
Certification Authorities*.

## 4. Install the first signed version by hand

The updater checks the copy that is running. A copy built before the secrets existed is unsigned,
so it can never update itself: download and install the first signed release once. From then on,
Help ▸ Check for updates installs new releases.

## Other platforms

- **Linux:** packages are not signed; updates check the published checksums. Nothing to set up.
- **macOS:** there is no free option. The updater accepts only a Developer ID signature (Apple
  Developer Program), and craft-update currently expects one fixed team ID, so a PSG Developer ID
  also needs craft-update to take the team ID from the app.
