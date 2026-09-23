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
version = "0.8.0"
```

Internal workspace dependencies reference `{ workspace = true }`, guaranteeing version lockstep across all crates.

---

## Release Process

### 1. Update Version
Edit the root `Cargo.toml` and increment the version string:

```toml
[workspace.package]
version = "0.8.1" # or target version
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
git commit -m "chore: bump version to v0.8.1"
```

### 4. Create an Annotated Tag
Create an annotated Git tag matching the SemVer format `vX.Y.Z`:

```powershell
git tag -a v0.8.1 -m "Release v0.8.1"
```

### 5. Push to GitHub
Push both the commit and tag to GitHub:

```powershell
git push origin main
git push origin v0.8.1
```

### 6. Automated GitHub Release
When a tag matching `v*.*.*` is pushed, the [Release Workflow](.github/workflows/release.yml) automatically:
1. Compiles the optimized release binary (`cargo build --release --bin cash`).
2. Bundles `cash.exe`, `README.md`, and `LICENSE` into `cash-vX.Y.Z-x86_64-pc-windows-msvc.zip`.
3. Computes the SHA-256 checksum file.
4. Creates a new GitHub Release with release notes and attaches the zip archive and checksum.
