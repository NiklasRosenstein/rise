# Release Process

This document describes how to publish a new version of Rise to crates.io.

## Overview

Rise uses a workspace-based versioning system where both `rise-backend` and `rise` share the same version number defined in the root `Cargo.toml`. Publishing is automated via GitHub Actions when you push a version tag.

## Prerequisites

### One-Time Setup

1. **Add crates.io API token to GitHub secrets:**
   - Generate a token at https://crates.io/me/tokens
   - Add it to your repository: Settings → Secrets and variables → Actions → New repository secret
   - Name: `CARGO_REGISTRY_TOKEN`

2. **Configure trusted publishing (future):**
   - Once crates.io's trusted publishing is fully available, configure it at https://crates.io/settings/tokens
   - Add this repository as a trusted publisher
   - Remove the `CARGO_REGISTRY_TOKEN` secret and environment variables from the workflow

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
2. Monitor progress at: https://github.com/yourusername/rise/actions
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
- The workflow waits 60 seconds for crates.io to update
- If this isn't enough, you may need to manually publish `rise`:

```bash
cd rise-cli
cargo publish --token YOUR_CRATES_IO_TOKEN
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
repository = "https://github.com/yourusername/rise"
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

## Future: Trusted Publishing

Once crates.io's trusted publishing is fully documented and stable:

1. Configure the trusted publisher on crates.io for this repository
2. Remove the `CARGO_REGISTRY_TOKEN` environment variable from `.github/workflows/publish-crates.yml`
3. cargo will automatically use the GitHub OIDC token for authentication
4. No API tokens needed in GitHub secrets

This will provide better security by eliminating long-lived credentials.
