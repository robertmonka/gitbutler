# Compile GitButler

Run from `/home/robert/dev/gitbutler`.

`/etc/nixos#gitbutler` sets `CHANNEL=nightly`, `VERSION=nightly`, `WSLENV=VERSION:CHANNEL`, `OS=linux`, and `CARGO_INCREMENTAL=1`.

## NixOS full local build

Do not use `pnpm tauri-for-release`: its `tauri-before-build-command.sh` runs desktop, then `gitbutler-git`, then `but` sequentially and compiles shared crates twice. Leave that upstream script alone. Match the Windows recipe below: overlap the desktop frontend with one `cargo` invocation for `gitbutler-git` / `but`, then inject and build `gitbutler-tauri`. `CARGO_INCREMENTAL=1` keeps later rebuilds from doing a full relink.

```bash
nix develop /etc/nixos#gitbutler -c bash -lc '
set -euo pipefail
export CHANNEL=nightly
export VERSION=nightly
export OS=linux
export CARGO_INCREMENTAL=1
export TAURI_CONFIG="$(tr -d "\n\t" < crates/gitbutler-tauri/tauri.conf.nightly-local.json)"
cargo build --release -p gitbutler-git -p but &
rust=$!
pnpm install --frozen-lockfile
pnpm build:desktop -- --mode nightly
wait "$rust"
bash ./crates/gitbutler-tauri/inject-git-binaries.sh
cargo build --release -p gitbutler-tauri --features "builtin-but packaged-but-distribution"
./target/release/but --version
'
```

Outputs: `target/release/gitbutler-tauri`, `target/release/but`.

### Sync binaries, agent skill, and agent steering (required)

After the build, apply the Home Manager activation from `/etc/nixos/configuration.nix`:

```bash
sudo nixos-rebuild switch --flake /etc/nixos#nixos
```

This step is required on NixOS — do **not** run `but skill install` or `but agent setup` by hand. Skills and workflow steering come from Home Manager.

What it does:

- `~/.local/bin/but` is a symlink to `target/release/but` (via `mkOutOfStoreSymlink` in Home Manager).
- `~/.local/bin/gitbutler-tauri` is a wrapper that sets `LD_LIBRARY_PATH` / GIO / GStreamer and execs `target/release/gitbutler-tauri`. The wrapper plus `system.extraDependencies` keep webkitgtk in the Nix store so weekly GC cannot break the GUI.
- Activation `gitbutlerAgentSkills` runs `but skill install --global --path …` for Claude, Codex, Cursor, and Kimi (not GitHub Copilot).
- Skill content comes from the **embedded skill inside the freshly built `but` binary** (`crates/but/skill/` at compile time), not from loose repo files.
- Activation computes the SHA-256 of `~/.local/bin/but` at runtime and compares it to `.but_skill_stamp` in each skill directory. When `but` is rebuilt, the hash changes and the next `nixos-rebuild switch` reinstalls the skill automatically. **No manual version bump is needed** (unlike `graphifyVersion` for the graphify skill).
- Activation also refreshes agent steering from `gitbutlerAgentPolicy` in `/etc/nixos/configuration.nix` into `~/.cursor/rules/gitbutler.mdc` and the Nix-managed blocks in Claude / Codex / Kimi instruction files. That policy keeps the hard local-only rules (no push / no PR / no `but land`, own branch, HARD BAN on other agents' WIP) and adds the chosen workflow preferences: amend small follow-ups into the matching commit, suggest splitting mixed commits, auto-update from the target with `but pull` (with `--check` when other agents' branches are applied), and commit checkpoints after each turn.

Verify after switch:

```bash
sha256sum /home/robert/dev/gitbutler/target/release/but
cat ~/.cursor/skills/gitbutler/.but_skill_stamp   # must match the hash above
but --version
but skill check
rg -n 'Amend local fixes|Update from the target branch|Commit checkpoints' ~/.cursor/rules/gitbutler.mdc
```

Expected: all four skill paths exist, `.but_skill_stamp` matches the current `but` hash, `but skill check` reports `[nightly]` for Claude/Codex/Cursor (Kimi is installed on disk; `skill check` may not list it), and `gitbutler.mdc` contains the amend / auto-update / checkpoint sections.

**Note:** `but skill check` compares only the version string (`nightly`), not file content. After a rebuild, always run `nixos-rebuild switch` — do not rely on `but skill check` alone to detect stale skill files.

## Windows full local build

Build in the native NTFS clone of this fork at `C:\webarm\gitbutler` (branch `gitea-native-integration`). Do not build from `\\wsl.localhost\...` — the WSL file bridge is slow and fragile. Do not wrap the Windows build in `nix develop`.

Run this in PowerShell 7. Call `pnpm.cmd` (not the `pnpm` PowerShell shim): the `.ps1` wrapper drops `--` and turbo then sees `--mode` as its own flag. Overlap the desktop frontend with `gitbutler-git` / `but` so those minutes are not added on top of Rust. `CARGO_INCREMENTAL=1` keeps later Windows rebuilds from doing a full MSVC relink of the 50–70 MB binaries.

Once, as Administrator, exclude the tree from Microsoft Defender realtime scanning (otherwise even a no-op release relink stays many minutes):

```powershell
Add-MpPreference -ExclusionPath C:\webarm\gitbutler
```

Before compiling, fast-forward the Windows clone from the fork (`origin` = `robertmonka/gitbutler`, branch `gitea-native-integration`). NixOS updates are out of scope here.

```bash
/mnt/c/Program\ Files/PowerShell/7/pwsh.exe -NoProfile -Command '
$ErrorActionPreference = "Stop"
$p = "C:\webarm\gitbutler"
$install = "$env:LOCALAPPDATA\GitButler\bin"
$gitbash = "C:\Program Files\Git\bin\bash.exe"
Set-Location $p
& "C:\Program Files\Git\cmd\git.exe" pull --ff-only origin gitea-native-integration
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$env:CHANNEL = "nightly"
$env:VERSION = "nightly"
$env:OS = "windows"
$env:CARGO_INCREMENTAL = "1"
$env:TAURI_CONFIG = Get-Content "$p\crates\gitbutler-tauri\tauri.conf.nightly-local.json" -Raw | ConvertFrom-Json | ConvertTo-Json -Compress -Depth 100
$rust = Start-Process -FilePath cargo -ArgumentList @("build","--release","-p","gitbutler-git","-p","but") -WorkingDirectory $p -NoNewWindow -PassThru
pnpm.cmd install --frozen-lockfile
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm.cmd build:desktop -- --mode nightly
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Wait-Process -Id $rust.Id
if ($rust.ExitCode -ne 0) { exit $rust.ExitCode }
& $gitbash ./crates/gitbutler-tauri/inject-git-binaries.sh
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo build --release -p gitbutler-tauri --features windows
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
New-Item -ItemType Directory -Force $install | Out-Null
Copy-Item -Force "$p\target\release\but.exe" "$install\but.exe"
Copy-Item -Force "$p\target\release\gitbutler-tauri.exe" "$install\gitbutler-tauri.exe"
& "$install\but.exe" --version
exit $LASTEXITCODE
'
```

Outputs: `C:\Users\robert\AppData\Local\GitButler\bin\gitbutler-tauri.exe`, `C:\Users\robert\AppData\Local\GitButler\bin\but.exe`. Intermediate artifacts: `C:\webarm\gitbutler\target\release\but.exe`, `C:\webarm\gitbutler\target\release\gitbutler-tauri.exe`.

### Sync agent skill (required)

After copying the binaries, install the GitButler agent skill for Claude, Codex, and Cursor from the freshly built `but.exe`:

```bash
/mnt/c/Program\ Files/PowerShell/7/pwsh.exe -NoProfile -Command '
$ErrorActionPreference = "Stop"
$but = "$env:LOCALAPPDATA\GitButler\bin\but.exe"
$stamp = (Get-FileHash $but -Algorithm SHA256).Hash.ToLower()
foreach ($agent in @("claude","codex","cursor")) {
  $dir = "$env:USERPROFILE\.$agent\skills\gitbutler"
  & $but skill install --global --path $dir
  Set-Content -NoNewline -Path "$dir\.but_skill_stamp" -Value $stamp
}
& $but skill check
'
```

What it does:

- Skill content comes from the **embedded skill inside the freshly built `but.exe`** (`crates/but/skill/` at compile time), not from loose repo files.
- `.but_skill_stamp` stores the SHA-256 of `but.exe` so you can skip reinstall when the binary has not changed.

Verify:

```powershell
$but = "$env:LOCALAPPDATA\GitButler\bin\but.exe"
(Get-FileHash $but -Algorithm SHA256).Hash.ToLower()
Get-Content "$env:USERPROFILE\.cursor\skills\gitbutler\.but_skill_stamp"  # must match
& $but --version
& $but skill check
```

Expected: all three skill paths report `[nightly]`, and `.but_skill_stamp` matches the current `but.exe` hash.

**Note:** `but skill check` compares only the version string (`nightly`), not file content. After a rebuild, always rerun the skill install step — do not rely on `but skill check` alone to detect stale skill files.

## Ubuntu WSL temporary full build

Build in a temporary Ubuntu checkout and install Ubuntu binaries:

```bash
tar --exclude='./target' --exclude='./.git' --exclude='./graphify-out' --exclude='./node_modules' --exclude='*/node_modules' -C /home/robert/dev/gitbutler -cf - . | nix develop /etc/nixos#gitbutler -c /mnt/c/Windows/System32/wsl.exe -d Ubuntu -u root --cd /tmp -- bash -lc '
set -euo pipefail
builddir=\$(mktemp -d /tmp/gitbutler-build.XXXXXX)
trap "rm -rf \"\$builddir\"" EXIT
tar -C "\$builddir" -xf -
cat > "\$builddir/build-ubuntu.sh" <<\UBUNTU_BUILD
#!/usr/bin/env bash
set -euo pipefail
export OS=linux
export TAURI_CONFIG="\$(tr -d "\\n\\t" < crates/gitbutler-tauri/tauri.conf.nightly-local.json)"
export CARGO_INCREMENTAL=1
cargo build --release -p gitbutler-git -p but &
rust=\$!
pnpm install --frozen-lockfile
pnpm build:desktop -- --mode nightly
wait "\$rust"
bash ./crates/gitbutler-tauri/inject-git-binaries.sh
cargo build --release -p gitbutler-tauri --features "builtin-but packaged-but-distribution"
UBUNTU_BUILD
chmod +x "\$builddir/build-ubuntu.sh"
chown -R robert:robert "\$builddir"
runuser -u robert -- bash -lc "cd \"\$builddir\" && ./build-ubuntu.sh"
install -m 0755 "\$builddir/target/release/but" /usr/local/bin/but
install -m 0755 "\$builddir/target/release/gitbutler-tauri" /usr/local/bin/gitbutler-tauri
/usr/local/bin/but --version
'
```

Outputs: Ubuntu `/usr/local/bin/gitbutler-tauri`, Ubuntu `/usr/local/bin/but`; the temporary `/tmp/gitbutler-build.*` directory is removed automatically.

## GitButler Lite (dev)

Lite is a separate Electron app (`apps/lite/`). It is **not** started by `but gui` (that opens `gitbutler-tauri`).

On NixOS/WSL, run from the repo flake shell (`direnv` / `nix develop`) so Electron can load system GUI libraries. Upstream `pnpm dev:lite` is Turborepo watch for `@gitbutler/lite`. If `@gitbutler/but-sdk` native bindings are missing, build them first: `pnpm build:sdk`.

```bash
cd /home/robert/dev/gitbutler
pnpm build:sdk   # when packages/but-sdk native .node bindings are missing
pnpm dev:lite
```

On Windows, run the same commands from `C:\webarm\gitbutler` (native NTFS clone, not `\\wsl.localhost\...`).
