#!/bin/bash
set -e

# Parse arguments
DRY_RUN=false
VERSION=""
COMMIT_RANGE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run|-n)
            DRY_RUN=true
            shift
            ;;
        *)
            if [ -z "$VERSION" ]; then
                VERSION="$1"
            else
                COMMIT_RANGE="$1"
            fi
            shift
            ;;
    esac
done

# Check if version argument is provided
if [ -z "$VERSION" ]; then
    echo "Usage: $0 [--dry-run] <version> [commit-range]"
    echo ""
    echo "Examples:"
    echo "  $0 0.1.4                    # Create release for version 0.1.4"
    echo "  $0 --dry-run 0.1.4          # Preview release notes for next version"
    echo "  $0 --dry-run 0.1.4 v0.13.0..HEAD  # Preview notes for specific range"
    exit 1
fi

TAG="v${VERSION}"

# Validate version format (basic check for X.Y.Z)
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: Version must be in format X.Y.Z (e.g., 0.1.4)"
    exit 1
fi

if [ "$DRY_RUN" = false ]; then
    echo "Updating version to ${VERSION}..."

    # Update version in Cargo.toml
    sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml && rm Cargo.toml.bak

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
else
    echo "DRY RUN MODE: Generating release notes preview for ${TAG}"
    echo ""
fi

# Generate release notes with Claude analysis
echo "Generating release notes..."

# Determine commit range
if [ -z "$COMMIT_RANGE" ]; then
    # Get the previous tag
    PREV_TAG=$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || echo "")

    if [ -z "$PREV_TAG" ]; then
        echo "No previous tag found, using all commits"
        COMMIT_RANGE="HEAD"
    else
        echo "Analyzing commits since ${PREV_TAG}..."
        COMMIT_RANGE="${PREV_TAG}..HEAD"
    fi
else
    echo "Using provided commit range: ${COMMIT_RANGE}"
fi

# Get commit messages
COMMITS=$(git log "${COMMIT_RANGE}" --pretty=format:"%h %s" --no-merges)

if [ -z "$COMMITS" ]; then
    echo "No commits found in range ${COMMIT_RANGE}"
    RELEASE_SUMMARY="No changes in this release."
else
    # Create a temporary file for the analysis prompt
    TEMP_PROMPT=$(mktemp)
    cat > "$TEMP_PROMPT" << EOF
Analyze the following Git commit messages and provide a concise summary for release notes. Focus on:
1. Breaking changes (if any) - mark these clearly with ⚠️ BREAKING CHANGE
2. New features
3. Bug fixes
4. Other notable changes

Be concise and user-focused. Use markdown formatting. Start with a brief overview, then list key changes.

Commits:
${COMMITS}
EOF

    # Call Claude API to generate summary
    RELEASE_SUMMARY=$(claude -p "$(cat "$TEMP_PROMPT")" 2>/dev/null || echo "Failed to generate AI summary")
    rm "$TEMP_PROMPT"
fi

if [ "$DRY_RUN" = true ]; then
    # Dry run mode: just print the release notes
    echo ""
    echo "=========================================="
    echo "Release Notes Preview for ${TAG}"
    echo "=========================================="
    echo ""
    echo "${RELEASE_SUMMARY}"
    echo ""
    echo "---"
    echo ""
    echo "## Full Changelog"
    echo ""
    echo "(GitHub auto-generated changelog would appear here)"
    echo ""
else
    # Create release notes file
    NOTES_FILE=$(mktemp)
    cat > "$NOTES_FILE" << EOF
${RELEASE_SUMMARY}

---

## Full Changelog
EOF

    # Create GitHub release with custom notes and auto-generated changelog
    echo "Creating GitHub release..."
    gh release create "${TAG}" --notes-file "$NOTES_FILE" --generate-notes

    # Clean up
    rm "$NOTES_FILE"

    echo ""
    echo "✓ Successfully created and pushed version ${VERSION} with tag ${TAG}"
    echo "✓ GitHub release created with AI-generated summary and auto-generated notes"
fi
