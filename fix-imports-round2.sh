#!/bin/bash
set -e

echo "=== Round 2: Fix remaining crate:: references ==="
echo ""

# Fix remaining server module references in db/ and server/
echo "Fixing crate::encryption:: → crate::server::encryption::"
find src/db src/server -name "*.rs" -type f -print0 | xargs -0 sed -i 's|crate::encryption::|crate::server::encryption::|g'

echo "Fixing crate::deployment:: → crate::server::deployment::"
find src/db src/server -name "*.rs" -type f -print0 | xargs -0 sed -i 's|crate::deployment::|crate::server::deployment::|g'

echo "Fixing crate::auth:: → crate::server::auth::"
find src/server -name "*.rs" -type f -print0 | xargs -0 sed -i 's|crate::auth::|crate::server::auth::|g'

echo "Fixing crate::oci:: → crate::server::oci::"
find src/server -name "*.rs" -type f -print0 | xargs -0 sed -i 's|crate::oci::|crate::server::oci::|g'

echo ""
echo "=== Round 2 fixes complete ==="
echo ""
git diff --stat src/
