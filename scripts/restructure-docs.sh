#!/bin/bash
set -e

echo "📁 Restructuring Rise documentation..."

# Move existing files to new structure
echo "Moving existing documentation files..."

# Introduction
echo "  → Moving introduction files..."
mv docs/introduction.md docs/introduction/README.md
mv docs/architecture.md docs/introduction/architecture.md

# Getting Started
echo "  → Moving getting-started files..."
mv docs/getting-started.md docs/getting-started/README.md

# Core Concepts
echo "  → Moving core concepts files..."
mv docs/authentication.md docs/core-concepts/authentication.md
mv docs/projects-teams.md docs/core-concepts/projects-teams.md

# Features
echo "  → Moving features files..."
mv docs/service-accounts.md docs/features/service-accounts.md
mv docs/registry.md docs/features/registry.md

# Development
echo "  → Moving development files..."
mv docs/contributing.md docs/development/contributing.md

echo "✅ File moves complete!"
echo ""
echo "📝 Files that still need manual editing:"
echo "  - docs/introduction/README.md (expand with 'why Rise?')"
echo "  - docs/introduction/architecture.md (REWRITE: problem/solution focused)"
echo "  - docs/getting-started/README.md (consolidate, remove duplicates)"
echo "  - docs/core-concepts/authentication.md (minor edits, remove duplicates)"
echo "  - docs/features/service-accounts.md (CONDENSE: 548→250 lines)"
echo "  - docs/features/registry.md (CONDENSE: 281→150 lines)"
echo "  - docs/development/contributing.md (update references)"
echo ""
echo "📄 New files to create manually:"
echo "  - docs/getting-started/cli-basics.md"
echo "  - docs/core-concepts/deployments.md"
echo "  - docs/features/web-frontend.md"
echo "  - docs/deployment/aws-ecr.md"
echo "  - docs/deployment/docker-local.md"
echo "  - docs/deployment/production.md"
echo "  - docs/development/database.md"
echo "  - docs/development/testing.md"
echo "  - docs/SUMMARY.md (update)"
echo "  - README.md (rewrite)"
