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
- `cash-test-harness`
- `xtask`

The authoritative version is declared in the root [Cargo.toml](Cargo.toml) under:

```toml
[workspace.package]
version = "0.9.1"
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
Run cargo check and test to verify clean compilation and update `Cargo.lock`:

```powershell
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

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
1. Compiles the optimized release binary (`cargo build --release --bin cash`).
2. Bundles `cash.exe`, `README.md`, and `LICENSE` into `cash-vX.Y.Z-x86_64-pc-windows-msvc.zip`.
3. Computes the SHA-256 checksum file.
4. Creates a new GitHub Release whose notes are the commits since the previous tag,
   grouped by type ([`.github/scripts/release-notes.ps1`](.github/scripts/release-notes.ps1)),
   and attaches the zip archive and checksum.
