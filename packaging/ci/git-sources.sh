#!/usr/bin/env bash
# Every git dependency comes from github.com/storytold, and nothing in cargo's config replaces,
# patches or overrides a source behind Cargo.lock's back. The shared gate doesn't check this, so
# .github/workflows/build.yaml runs it in its first job. Fails when Cargo.lock is out of date.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
test -f Cargo.lock || { echo "Cargo.lock is missing"; exit 1; }
# Source replacement, patches and path overrides in cargo config never show up in Cargo.lock.
# Parsed rather than grepped: TOML has many spellings of one key. Cargo still reads the legacy
# .cargo/config.
python3 - <<'PY'
import os, sys, tomllib
for path in (".cargo/config", ".cargo/config.toml"):
    if not os.path.exists(path):
        continue
    config = tomllib.load(open(path, "rb"))
    replaced = [name for name, source in config.get("source", {}).items()
                if isinstance(source, dict) and "replace-with" in source]
    if replaced or "patch" in config or "paths" in config:
        print(f"{path} replaces, patches or overrides a source")
        sys.exit(1)
PY
cargo metadata --locked --format-version 1 >/dev/null
python3 - <<'PY'
import sys, tomllib
lock = tomllib.load(open("Cargo.lock", "rb"))
bad = sorted({p["source"] for p in lock.get("package", [])
              if p.get("source", "").startswith("git+")
              and not p["source"].startswith("git+https://github.com/storytold/")})
if bad:
    print("Git dependencies outside github.com/storytold:", *bad, sep="\n")
    sys.exit(1)
PY
echo "git dependencies: all from github.com/storytold"
