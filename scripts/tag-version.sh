#!/bin/bash
set -e

# Check if version argument is provided
if [ -z "$1" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.1.4"
    exit 1
fi

VERSION=$1
TAG="v${VERSION}"

# Validate version format (basic check for X.Y.Z)
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: Version must be in format X.Y.Z (e.g., 0.1.4)"
    exit 1
fi

echo "Updating version to ${VERSION}..."

# Update version in workspace Cargo.toml
sed -i "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml

# Update Cargo.lock
echo "Updating Cargo.lock..."
cargo update --workspace

# Show the changes
echo ""
echo "Changes to be committed:"
git diff Cargo.toml Cargo.lock

# Confirm before proceeding
read -p "Proceed with commit and tag? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted. Rolling back changes..."
    git checkout Cargo.toml Cargo.lock
    exit 1
fi

# Commit the changes
echo "Committing version bump..."
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to ${VERSION}"

# Create the tag
echo "Creating tag ${TAG}..."
git tag -a "${TAG}" -m "Release ${TAG}"

# Push commit and tag
echo "Pushing to remote..."
git push origin main
git push origin "${TAG}"

echo ""
echo "✓ Successfully created and pushed version ${VERSION} with tag ${TAG}"
