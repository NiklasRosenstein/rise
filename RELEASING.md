# Release Process

This document describes how to publish a new version of Rise to crates.io.

## Overview

Rise uses a workspace-based versioning system where both `rise-backend` and `rise` share the same version number defined in the root `Cargo.toml`. Publishing is automated via GitHub Actions when you push a version tag.

## Prerequisites

### One-Time Setup

**Trusted publishing is already configured!** This repository uses GitHub OIDC tokens for authentication via the `rust-lang/crates-io-auth-action@v1`.

No manual token setup is required. The workflow automatically obtains a temporary token from crates.io using the GitHub Actions OIDC identity.

## Release Steps

### 1. Update Version

Edit the version in the workspace `Cargo.toml`:

```toml
[workspace.package]
version = "0.2.0"  # Update this line
```

### 2. Update Changelog (if applicable)

Document notable changes in `CHANGELOG.md` or similar.

### 3. Commit and Create Tag

```bash
# Commit the version bump
git commit -am "chore: bump version to 0.2.0"

# Create an annotated tag
git tag -a v0.2.0 -m "Release v0.2.0"

# Push commits and tag
git push origin main
git push origin v0.2.0
```

### 4. Monitor the Release

1. GitHub Actions will automatically trigger when the tag is pushed
2. Monitor progress at: https://github.com/NiklasRosenstein/rise/actions
3. The workflow will:
   - Verify the tag version matches Cargo.toml
   - Build and publish `rise-backend` first
   - Wait for crates.io index to update
   - Build and publish `rise`
   - Create a GitHub release with notes

### 5. Verify Publication

Check that both crates are published:
- https://crates.io/crates/rise-backend
- https://crates.io/crates/rise

## Troubleshooting

### Version Mismatch Error

If the workflow fails with a version mismatch:
- Ensure the tag version (e.g., `v0.2.0`) matches the version in `Cargo.toml` (e.g., `0.2.0`)
- Delete the tag locally and remotely, fix the version, and recreate the tag

```bash
git tag -d v0.2.0
git push origin :refs/tags/v0.2.0
```

### Publish Failures

If `rise` fails to publish because it can't find the new `rise-backend` version:
- The workflow uses `cargo publish --workspace` which handles dependency ordering automatically
- If you need to manually publish, ensure you're authenticated first:

```bash
# Manual publish (only needed in rare cases)
cargo publish -p rise-backend
# Wait for crates.io index to update...
cargo publish -p rise
```

### Duplicate Version Error

If you accidentally publish the wrong version:
- You **cannot** republish the same version number
- You must yank the bad version and publish a new patch version:

```bash
cargo yank --vers 0.2.0 rise-backend
cargo yank --vers 0.2.0 rise
# Then bump to 0.2.1 and republish
```

## Workspace Version Management

All crates in the workspace inherit their version from `[workspace.package]` in the root `Cargo.toml`:

```toml
[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/NiklasRosenstein/rise"
```

Individual crate `Cargo.toml` files reference this:

```toml
[package]
name = "rise-backend"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
```

This ensures both crates always have the same version number, simplifying releases.

## Trusted Publishing

This repository uses **trusted publishing** via GitHub Actions OIDC tokens, providing better security by eliminating long-lived credentials.

The CI workflow (`.github/workflows/ci.yml`) automatically:
1. Authenticates with crates.io using the `rust-lang/crates-io-auth-action@v1`
2. Obtains a temporary publish token via GitHub's OIDC identity
3. Publishes both workspace crates in dependency order

No manual API tokens or secrets are required.
