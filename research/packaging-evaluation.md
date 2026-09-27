# Installing cash: Scoop, winget, Chocolatey, and the tool links on PATH

## Status

**Decided 2026-09-28** (ROADMAP item 18, refining spec D38 and D65). The question
started as "is `cash --link-tools` tested, and should cash put its folder on PATH?" and
widened to how cash gets installed at all, as a normal user without admin. Decided with
the user, by pick list:

| Question | Decision |
|---|---|
| Channels, and their order | **Scoop only, for now**, from a bucket of the author's own. winget later; Chocolatey not planned |
| Installer for the later winget package | **Inno Setup**, per-user (`PrivilegesRequired=lowest`) |
| The tool links folder on PATH | **Opt-in**, and when chosen, at the **front of the user PATH** |
| Who writes PATH | **cash itself** (a `--link-tools` option and its undo), so every channel shares one tested implementation |
| Public download URLs | **Make `tomcoolpxl/cash` public** |
| Where the bucket lives | **`tomcoolpxl/scoop-bucket`**, one personal bucket, updated by its own scheduled action |
| Code signing | **Unsigned now**; apply to the SignPath Foundation once cash has visible users |
| Windows Terminal profile | **Written on every install** (a JSON fragment), removed on uninstall, as D38 said |

Not asked, so left as it is: x64 only; an ARM64 build waits for a machine to test it on.

**Implemented 2026-09-28**, items 1 to 5 of [What follows](#what-follows-from-the-decisions):
the static C runtime and its release check, the icon and version resource, `--add-to-path`
and `--unlink-tools`, `--terminal-profile` and its removal, the licence notices in the
zip, and [`packaging/scoop/cash.json`](../packaging/scoop/cash.json), which is valid
against Scoop's schema. Left to the author: making the repository public, creating the
bucket from Scoop's template, and a release that carries these commands.

## What exists today

- `release.yml` builds `cash.exe` on `windows-latest`, zips it with `README.md` and
  `LICENSE`, and publishes the zip and its SHA-256 on the GitHub release.
- `cash --link-tools [DIR]` (D65) makes 126 hard links to `cash.exe`. Run from the
  release `cash.exe` on 2026-09-27 into a scratch folder: all 126 made, and `sort.exe`
  sorted from PowerShell, outside cash.

Found while checking, all to fix whatever the channel:

- **The C runtime is not linked in.** The release `cash.exe` imports `VCRUNTIME140.dll`,
  so it fails on a Windows without the Visual C++ Redistributable. A winget dependency on
  `Microsoft.VCRedist` is a machine-wide install (admin), and CoolWSL's winget PR
  (`microsoft/winget-pkgs#368818`) already failed validation on a package dependency.
  Build with `+crt-static`, as ripgrep does.
- **No icon and no version resource** in `cash.exe`: Explorer, Task Manager, Apps &
  features and SmartScreen all show a bare file.
- **Notices missing from the zip.** `NOTICE` (brush, MIT) and `licenses/` (lsd,
  Apache-2.0) are not in it, nor the licences of the statically linked crates.
- **`Cargo.toml` names `github.com/thraa/cash`**; the repository is
  `github.com/tomcoolpxl/cash`.
- **`cash` is taken on crates.io** (a currency library), so D38's `cargo install cash`
  would need another crate name.
- **The repository is private**, so no package manager can download its releases.

### `--link-tools` and its tests

Eight integration tests (`crates/cash/tests/link_tools.rs`) and one unit test. Not
covered: the default folder (every test names DIR), the "already on PATH" message, links
removed for tools cash no longer carries, and the System32 note.

One real gap: a link that `remove_file` cannot delete stopped the whole refresh halfway,
through the `?`, before the manifest was written. Measured on Windows 11 (26200):
Windows deletes a name of a running program while the file has other names, but refuses
the last one (and any write to it); renaming it works either way. So a refresh fails
when the old `cash.exe` is itself gone (`scoop cleanup`, a deleted `cash.exe.old`) and a
program outside cash still runs one of its links: whichever link is replaced last is
the file's last name. Such a link can be renamed aside and deleted by a later run.

The PowerShell line it prints to add the folder to PATH is harmful: .NET reads the user
`Path` with `GetValue`, which expands `%VAR%` entries, and writes it with `SetValue`,
which stores a plain `REG_SZ`
([dotnet/runtime#1442](https://github.com/dotnet/runtime/issues/1442), closed as not
planned). On the author's machine the user `Path` is `REG_EXPAND_SZ` and ends in
`%USERPROFILE%\go\bin`; one run of the printed line would flatten it.

### Where a user PATH entry lands

Windows builds a process's `Path` as the machine `Path`, then the user `Path`. A user
entry therefore always comes after System32, whatever its place in the user list. That is
the safe order: a `.bat` file calling `find`, `sort` or `timeout` keeps Windows' tools.

On the author's machine, 95 of the 126 tool names already exist on PATH:

- 13 in the machine `Path`, which no user entry can beat: System32's `expand find hostname
  more ping reset sort timeout tree where whoami`, and nvm's `elevate` and `install`;
- 82 in `scoop\shims`, all BusyBox applets (`ls.shim` is `busybox.exe` with `args = ls`).

Appended to the end of the user `Path`, as the printed line does, the links folder would
give programs outside cash only about 31 of cash's tools; at the front, all but the 13.
Inside cash none of this matters: cash answers for its own tools first. `link.exe` is
also the MSVC linker's name, one more reason the links stay opt-in.

## The channels

### Scoop

- Per-user by design: everything under `~\scoop`, never admin.
- A bucket is a public git repository of JSON manifests. The author's own bucket needs no
  review. The main bucket wants a "well-known and widely used developer tool (e.g. … at
  least 500 stars and 150 forks)" and "no elaborate pre/post install scripts"
  ([criteria](https://github.com/ScoopInstaller/Scoop/wiki/Criteria-for-including-apps-in-the-main-bucket)).
- The release zip works as it is. `checkver: github` and an `autoupdate` URL template let
  the bucket's scheduled action (Scoop's bucket template, "Excavator") update the manifest
  from new releases, with no token in the cash repository.
- `bin: "cash.exe"` puts a `cash` shim in `~\scoop\shims`. The tools must **not** be
  declared as shims: BusyBox owns 82 of those names on the author's machine, and two apps
  claiming a shim overwrite each other.
- Each version is unpacked in its own folder and `current` is a junction to it, so an
  upgrade never overwrites a running `cash.exe`. Scoop still refuses by default ("The
  following instances … are still running"); `scoop config ignore_running_processes true`
  lets it proceed ([PR #4713](https://github.com/ScoopInstaller/Scoop/pull/4713); a
  kill option is only requested, [#5617](https://github.com/ScoopInstaller/Scoop/issues/5617)).
- `persist: "bin"` keeps a `bin` folder in `~\scoop\persist\cash` across versions and
  junctions it into each version folder. That is where "`bin` next to `cash.exe`" lands,
  so the links folder, and a PATH entry for it, survive upgrades; `post_install`
  refreshes the links when a manifest (`.cash-links`) is there.

### winget

From the 1.12 manifest schema, the winget-cli source and learn.microsoft.com:

- winget passes no scope switch of its own; per-user comes from the installer, marked
  `Scope: user` on each installer entry. A manifest whose entries are all `Scope: user`
  offers only that.
- **Portable (a zip with a nested portable exe)** is the easiest package, but an upgrade
  deletes the old file and moves the new one in, with no rename aside
  ([PortableInstaller.cpp](https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerCLICore/PortableInstaller.cpp)).
  With `cash.exe` running, which for a shell is always, `winget upgrade cash` fails with
  "Access is denied" ([#5235](https://github.com/microsoft/winget-cli/issues/5235)).
  Uninstall leaves files the program made (the links) unless `--purge`.
- **The installer type cannot change later** without breaking upgrades, so the first
  manifest must use the real installer.
- New packages get a human review; waits of weeks were reported in September 2026
  ([#435209](https://github.com/microsoft/winget-pkgs/issues/435209)). Later versions
  can be submitted by `vedantmgoyal9/winget-releaser` (Komac underneath) with a classic
  PAT (`public_repo`), but the first version is submitted by hand.
- winget-pkgs flags a new version whose properties differ from the published one
  (`Manifest-Metadata-Consistency`, as on CoolWSL's `microsoft/winget-pkgs#369148`).
- Code signing is not required ([discussion #4327](https://github.com/microsoft/winget-cli/discussions/4327)).

### Chocolatey

Possible, and not for this audience: Chocolatey lives in `C:\ProgramData\chocolatey`
and puts its shims on the machine `Path`, so its normal setup needs admin; it makes a
shim for every exe in a package (the links would each need an `.ignore` file); and its
community repository moderates every package. A fit later only for machines already
managed with Chocolatey.

### dist (formerly cargo-dist)

Its MSI is per-machine (`InstallScope='perMachine'`, system `Path`), its PowerShell
installer cannot replace a running exe and registers no uninstaller, and it does not
publish to winget. It does read and write the user `Path` correctly
(`DoNotExpandEnvironmentNames`, `ExpandString`), which is the pattern cash follows.

## The installer, for winget later

| | Inno Setup 6.7 | WiX 7 MSI, per-user | NSIS |
|---|---|---|---|
| No-admin per-user install | `PrivilegesRequired=lowest`, `{autopf}` = `%LOCALAPPDATA%\Programs` | `perUserOrMachine` with `MSIINSTALLPERUSER=1`; pure `perUser` raises ICE warnings | `RequestExecutionLevel user` |
| Replacing a running `cash.exe` | `CloseApplications=no`, then rename it aside in `PrepareToInstall` | moved to the rollback cache; exit 3010, "restart required" in winget | `Rename` by hand |
| On the runners | preinstalled (6.7.1) on every Windows image | `dotnet tool install --global wix`, and CI must accept the WiX 7 EULA (OSMF) | only on windows-2022 |
| winget type | `inno` | `wix` | `nullsoft` |

Inno's defaults would hurt: `CloseApplications=yes` closes running applications in a
silent install, and Restart Manager ends a console program with Ctrl-C, which a shell
ignores until it is killed. `restartreplace` needs admin. Old `cash-old-*.exe` copies
cannot be deleted at reboot without admin, so the next install or cash itself sweeps them.

## Signing

- Not required by winget or Scoop. winget re-marks downloads from its default source as
  trusted, so installs through it show no SmartScreen prompt; a browser download of an
  unsigned file does.
- **Smart App Control** blocks an unsigned executable outright where it is on (by default
  only on clean installs of Windows 11 22H2 and later), with no override. Hard links are
  the same file, so they carry `cash.exe`'s signature, or lack of one.
- Unsigned Rust binaries draw machine-learning Defender detections now and then; false
  positives go to <https://www.microsoft.com/wdsi/filesubmission>.
- **Azure Artifact Signing** (Trusted Signing until January 2026) takes individuals only
  in the US and Canada ([quickstart](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart)).
- **SignPath Foundation**: free for OSI-licensed projects built on GitHub-hosted runners,
  but wants an established user base; the certificate names SignPath, not the author
  ([terms](https://signpath.org/terms.html)).
- **Certum Open Source Code Signing**: about €49 a year in the cloud (SimplySign), shown as
  "Open Source Developer, name"; signing from CI needs unofficial one-time-code tools.

## What follows from the decisions

For cash itself:

1. Link the C runtime statically; check in CI that `cash.exe` imports no `VCRUNTIME140.dll`.
2. An icon and a version resource in `cash.exe`.
3. `--link-tools` renames a link Windows will not delete aside instead of stopping; an
   option to put the links folder at the front of the user `Path`, keeping
   `REG_EXPAND_SZ` and `%VAR%` entries, adding it once, and announcing the change
   (`WM_SETTINGCHANGE`); and an undo that removes the links, the entry, and nothing else.
4. A command that writes, and one that removes, the Windows Terminal fragment in
   `%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\cash\`, pointing at the
   `cash.exe` it was run as (Scoop's `current` path, not a version folder).
5. The zip carries `NOTICE`, `licenses/` and the third-party licences.
6. `Cargo.toml`'s repository URL.

Outside the repository, for the author: make `tomcoolpxl/cash` public, and create
`tomcoolpxl/scoop-bucket` holding `bucket/cash.json`.
