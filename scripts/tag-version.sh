#!/bin/bash
set -e

# Check prerequisites
check_prerequisites() {
    local missing=()

    if ! command -v git &> /dev/null; then
        missing+=("git")
    fi

    if ! command -v cargo &> /dev/null; then
        missing+=("cargo")
    fi

    if ! command -v claude &> /dev/null; then
        missing+=("claude (Claude CLI - optional for AI-generated release notes)")
    fi

    if [ ${#missing[@]} -gt 0 ]; then
        echo "Error: Missing required tools:"
        for tool in "${missing[@]}"; do
            if [[ "$tool" == *"optional"* ]]; then
                echo "  - $tool"
            else
                echo "  ✗ $tool"
            fi
        done
        echo ""
        echo "Install missing tools:"
        echo "  - claude: https://github.com/anthropics/anthropic-tools"
        exit 1
    fi
}

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

# Check prerequisites before proceeding
check_prerequisites

TAG="v${VERSION}"

# Validate version format (basic check for X.Y.Z)
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: Version must be in format X.Y.Z (e.g., 0.1.4)"
    exit 1
fi

# Check for uncommitted changes (only in non-dry-run mode)
if [ "$DRY_RUN" = false ]; then
    if ! git diff-index --quiet HEAD --; then
        echo "Error: You have uncommitted changes in your working directory"
        echo "Please commit or stash your changes before creating a release"
        git status --short
        exit 1
    fi

    # Check if on develop branch
    CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
    if [ "$CURRENT_BRANCH" != "develop" ]; then
        echo "Warning: You are not on the develop branch (current: $CURRENT_BRANCH)"
        read -p "Continue anyway? (y/n) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            echo "Aborted."
            exit 1
        fi
    fi
fi

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

# Generate release notes with Claude analysis
echo "Generating release notes..."

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
    RELEASE_SUMMARY=$(claude -p "$(cat "$TEMP_PROMPT")" 2>/dev/null || echo "Failed to generate AI summary. Please install Claude CLI from https://github.com/anthropics/anthropic-tools")
    rm "$TEMP_PROMPT"
fi

# DRY RUN: Just print the release notes and exit
if [ "$DRY_RUN" = true ]; then
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
    exit 0
fi

# REAL RUN: Show summary and ask for confirmation
echo ""
echo "=========================================="
echo "Release Plan for ${TAG}"
echo "=========================================="
echo ""
echo "The following actions will be performed:"
echo "  1. Update version in Cargo.toml to ${VERSION}"
echo "  2. Update Cargo.lock"
echo "  3. Commit changes with message: 'chore: bump version to ${VERSION}'"
echo "  4. Create git tag: ${TAG} (with AI-generated notes in tag annotation)"
echo "  5. Push commit and tag to origin"
echo ""
echo "Release notes preview:"
echo "---"
echo "${RELEASE_SUMMARY}"
echo "---"
echo ""

read -p "Proceed with release? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Release cancelled."
    exit 1
fi

# ============================================================================
# MUTATION ZONE: All changes happen below this point
# ============================================================================

echo ""
echo "Creating release ${VERSION}..."
echo ""

# Step 1: Update version in Cargo.toml
echo "[1/5] Updating Cargo.toml..."
sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml && rm Cargo.toml.bak

# Step 2: Update Cargo.lock
echo "[2/5] Updating Cargo.lock..."
cargo update --workspace --quiet

# Step 3: Commit the changes
echo "[3/5] Committing version bump..."
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to ${VERSION}"

# Step 4: Create the tag with AI-generated notes in the annotation
echo "[4/5] Creating tag ${TAG}..."
git tag -a "${TAG}" -m "${RELEASE_SUMMARY}"

# Step 5: Push commit and tag
echo "[5/5] Pushing to remote..."
git push origin develop
git push origin "${TAG}"

echo ""
echo "✓ Successfully created and pushed version ${VERSION} with tag ${TAG}"
echo "✓ AI-generated release notes stored in tag annotation"
echo ""
echo "CI will now:"
echo "  - Build release artifacts via cargo-dist"
echo "  - Create the GitHub release"
echo "  - Update release notes from tag annotation (update-release-notes workflow)"
