# Releasing Cash

Cash follows [Semantic Versioning 2.0.0](https://semver.org/):

- **MAJOR** (`X.0.0`): Incompatible API or breaking behavioral changes.
- **MINOR** (`0.X.0`): Backwards-compatible new features and capabilities.
- **PATCH** (`0.0.X`): Backwards-compatible bug fixes and optimizations.

---

## Workspace Versioning Architecture

The Cash project uses Cargo workspace package inheritance (`version.workspace = true`) to maintain a single unified version across all workspace crates:

- `cash` (CLI binary)
- `cash-win32`
- `cash-core`
- `cash-shell`
- `cash-parser`
- `cash-builtins`
- `cash-coreutils-builtins`
- `cash-interactive`
- `cash-sed`
- `cash-awk`
- `cash-bc`
- `cash-test-harness`
- `xtask`

The vendored crates under `vendor/` keep their own versions.

The authoritative version is declared in the root [Cargo.toml](Cargo.toml) under:

```toml
[workspace.package]
version = "X.Y.Z"
```

Internal workspace dependencies reference `{ workspace = true }`, guaranteeing version lockstep across all crates.

---

## Commit messages

Commit subjects follow [Conventional Commits](https://www.conventionalcommits.org/), because
the release notes are built from them:

```text
type(scope): description
```

| Type | Use it for | Release notes section |
| --- | --- | --- |
| `feat` | a new command, option or behaviour | Features |
| `fix` | a bug fix, including a behaviour brought in line with bash | Bug fixes |
| `perf` | a speed-up with no change in behaviour | Performance |
| `docs` | spec, roadmap, README and research documents | Documentation |
| `refactor` | restructuring with no change in behaviour | Refactoring |
| `test`, `ci`, `build`, `chore`, `style` | tests alone, workflows, the build, releases, formatting | Maintenance |

- The **scope** is optional and names the area: a builtin (`kill`, `cd`), a crate (`win32`,
  `parser`) or a subsystem (`completion`, `msys`). It shows in bold in the notes.
- The **description** is written for the reader of the release notes: what changed, as
  it will read there — `fix(kill): stop TERM from reaching every process on the console`.
  It is kept exactly as written, so start it lowercase unless it begins with a name.
- `!` before the colon (`feat(parser)!: …`) also lists the commit under **Breaking
  changes**; say what breaks in the body.
- A subject without a known type still appears, under **Other**. History from before
  this convention is mostly like that.

To preview the notes of the next release, before tagging:

```powershell
pwsh .github/scripts/release-notes.ps1 -Tag vNEXT -Out release-notes.md
```

---

## Release Process

### 1. Update Version
Edit the root `Cargo.toml` and increment the version string:

```toml
[workspace.package]
version = "0.9.2" # or target version
```

### 2. Update Lockfile and Verify Workspace
Run the checks CI runs, which also updates `Cargo.lock`:

```powershell
cargo deny check advisories
cargo xtask ci full
cargo xtask check lint --all-features
```

`cargo deny` (`cargo binstall cargo-deny`) fails on a crate RustSec lists or that was
yanked (`deny.toml`); `ci full` runs both lanes of tests (`.config/nextest.toml`) and
the doc tests. The release workflow runs CI's two lanes again on the
tag's commit before it builds anything (`release.yml` calls `ci.yml`).

Verify that cash reports the new version:

```powershell
cargo run --bin cash -- --version
```

### 3. Commit the Version Bump
```powershell
git add Cargo.toml Cargo.lock
git commit -m "chore(release): 0.9.2"
```

### 4. Create an Annotated Tag
Create an annotated Git tag matching the SemVer format `vX.Y.Z`:

```powershell
git tag -a v0.9.2 -m "cash 0.9.2"
```

### 5. Push to GitHub
Push both the commit and tag to GitHub:

```powershell
git push origin main
git push origin v0.9.2
```

### 6. Automated GitHub Release
When a tag matching `v*.*.*` is pushed, the [Release Workflow](.github/workflows/release.yml) automatically:
1. Compiles the shipped binary with the fully optimized `dist` profile (`cargo build
   --profile dist --bin cash`: fat LTO, one codegen unit) and checks that it imports no C
   runtime DLL. The `release` profile, for local builds, trades a little speed for a much
   quicker build.
2. Measures it against [`perf-budget.toml`](perf-budget.toml) with `cargo xtask perf`:
   its size, and cash's own start (the median of 30 `cash -c true` less that of a bare
   `cmd /c exit`). The figures go in the job's summary; a build over a limit stops the
   release. A release that needs more raises the limit in a commit of its own that says
   why. `cargo xtask perf --exe target/release/cash.exe` gives the same figures locally,
   for a less optimized build.
3. Bundles `cash.exe`, `README.md`, `LICENSE`, `NOTICE`, `licenses/` and
   `THIRD-PARTY-LICENSES.html` (written by `cargo about` from
   [`.github/about`](.github/about)) into `cash-vX.Y.Z-x86_64-pc-windows-msvc.zip`.
4. Computes the SHA-256 checksum file.
5. Compiles the per-user installer, `cash-vX.Y.Z-setup.exe`, from the same files with
   Inno Setup ([`packaging/inno/cash.iss`](packaging/inno/cash.iss)), with its own
   `.sha256`. It installs under `%LOCALAPPDATA%\Programs\cash\X.Y.Z` and runs
   `cash --install-finish`; `cash --update` on an installed cash fetches the next
   release's zip from the same page, so the zip's name must stay as it is.
6. Creates a new GitHub Release whose notes are the commits since the previous tag,
   grouped by type ([`.github/scripts/release-notes.ps1`](.github/scripts/release-notes.ps1)),
   and attaches the zip, the installer and both checksums.

### 7. Scoop
Nothing to do. The bucket, [`tomcoolpxl/scoop-bucket`](https://github.com/tomcoolpxl/scoop-bucket),
runs Scoop's Excavator action every few hours: `checkver` finds the new release, and
`autoupdate` points `bucket/cash.json` at its zip and `.sha256`. So the zip's name must stay
`cash-vX.Y.Z-x86_64-pc-windows-msvc.zip`. The manifest's source is
[`packaging/scoop/cash.json`](packaging/scoop/cash.json); a change to its hooks or notes
is copied to the bucket by hand (`python scripts/sync-bucket.py PATH\to\bucket\cash.json`
copies every field but the version, url and hash).

### 8. winget
From the installer (`InstallerType: inno`, `Scope: user`), package
`tomcoolpxl.cash`. The templates are in [`packaging/winget`](packaging/winget), with
`@VERSION@` and `@SHA256@` to replace (the hash from the setup's `.sha256` asset; the
tokens are deliberately not bare words, since `ManifestVersion` and `InstallerSha256`
contain them). Write the three filled files to `packaging/winget/out` (ignored by git),
validate with `winget validate --manifest packaging\winget\out` (schema 1.6.0, which
every winget accepts), test with `winget install --manifest packaging\winget\out` on a
machine without a Scoop cash (the setup refuses to run beside one), and open the pull
request on `microsoft/winget-pkgs` by hand for the first version (`wingetcreate
submit`, or a fork and PR), under `manifests/t/tomcoolpxl/cash/X.Y.Z/`. Later versions can go through
`vedantmgoyal9/winget-releaser` in the release workflow. The installer type cannot change
once published, so the first submission waits until the installer has shipped in a
release and been installed on a clean machine.
