#!/bin/sh
set -e

# Check if a superuser already exists
# (There's no direct command to check, so we'll try to create it and expect it to fail if it exists)
echo "Attempting to create superuser 'admin@example.com'..."
/usr/local/bin/pocketbase superuser upsert 'admin@example.com' 'admin123' || echo "Superuser already exists or an error occurred."

# Execute the main command to start the server
exec /usr/local/bin/pocketbase serve --http=0.0.0.0:8090
