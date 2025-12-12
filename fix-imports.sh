#!/bin/bash
set -e

echo "=== Comprehensive Import Path Fix Script ==="
echo ""

# Fix 1: db is at root level, not in server
echo "Fixing crate::server::db:: → crate::db::"
find src -name "*.rs" -type f -print0 | xargs -0 sed -i 's|crate::server::db::|crate::db::|g'

# Fix 2: Double server:: references
echo "Fixing crate::server::server:: → crate::server::"
find src -name "*.rs" -type f -print0 | xargs -0 sed -i 's|crate::server::server::|crate::server::|g'

# Fix 3: In server/mod.rs, the use statements that got double-prefixed
echo "Fixing server/mod.rs use statements"
sed -i 's|use crate::server::server::|use crate::server::|g' src/server/mod.rs

echo ""
echo "=== Import fixes complete ==="
echo ""
echo "Summary of changes:"
git diff --stat src/
