#!/bin/sh
set -e

PB_EXEC="/usr/local/bin/pocketbase"
POCKETBASE_URL="http://localhost:8090"
ADMIN_EMAIL="${POCKETBASE_ADMIN_EMAIL:-admin@example.com}"
ADMIN_PASSWORD="${POCKETBASE_ADMIN_PASSWORD:-admin123}"
TEST_USER_EMAIL="${POCKETBASE_TEST_USER_EMAIL:-test@example.com}"
TEST_USER_PASSWORD="${POCKETBASE_TEST_USER_PASSWORD:-test1234}"

# Start PocketBase in the background
${PB_EXEC} serve --http=0.0.0.0:8090 &
PB_PID=$!

# Wait for the server to be ready
echo "Waiting for PocketBase to start..."
for i in $(seq 1 30); do
    if wget --quiet --tries=1 --spider ${POCKETBASE_URL}/api/health 2>/dev/null; then
        echo "PocketBase is ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "PocketBase failed to start within 30 seconds"
        exit 1
    fi
    sleep 1
done

# Create admin superuser (idempotent - upsert will create or update)
echo "Creating admin superuser..."
${PB_EXEC} superuser upsert ${ADMIN_EMAIL} ${ADMIN_PASSWORD} 2>&1 || echo "Admin user creation/update completed"

# Create test user using PocketBase admin API
echo "Creating test user..."
# Wait a moment for things to settle
sleep 1

# Create an admin via the API (this creates an admin record, not a superuser)
# First, check if we need to create the _superusers collection or use the CLI-created superuser
# We'll create a regular admin account that can authenticate via API
ADMIN_CREATE_RESPONSE=$(curl -s -X POST ${POCKETBASE_URL}/api/collections/_superusers/records \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"${ADMIN_EMAIL}\",\"password\":\"${ADMIN_PASSWORD}\",\"passwordConfirm\":\"${ADMIN_PASSWORD}\"}" 2>&1)

# Now try to authenticate
AUTH_RESPONSE=$(curl -s -X POST ${POCKETBASE_URL}/api/collections/_superusers/auth-with-password \
    -H "Content-Type: application/json" \
    -d "{\"identity\":\"${ADMIN_EMAIL}\",\"password\":\"${ADMIN_PASSWORD}\"}")

ADMIN_TOKEN=$(echo "$AUTH_RESPONSE" | jq -r '.token // empty')

if [ -z "$ADMIN_TOKEN" ] || [ "$ADMIN_TOKEN" = "null" ] || [ "$ADMIN_TOKEN" = "empty" ]; then
    echo "Warning: Could not authenticate as admin (API method failed)"
    echo "Auth response: $AUTH_RESPONSE"
    echo "Test user creation skipped - will need manual creation or API access"
else
    echo "Admin authenticated successfully"

    # Create the test user in the users collection
    RESULT=$(curl -s -X POST ${POCKETBASE_URL}/api/collections/users/records \
        -H "Authorization: ${ADMIN_TOKEN}" \
        -H "Content-Type: application/json" \
        -d "{\"email\":\"${TEST_USER_EMAIL}\",\"password\":\"${TEST_USER_PASSWORD}\",\"passwordConfirm\":\"${TEST_USER_PASSWORD}\",\"emailVisibility\":true,\"verified\":true}")

    if echo "$RESULT" | jq -e '.id' >/dev/null 2>&1; then
        echo "Test user created successfully"
    elif echo "$RESULT" | jq -e '.message' >/dev/null 2>&1; then
        ERROR_MSG=$(echo "$RESULT" | jq -r '.message')
        if echo "$ERROR_MSG" | grep -qi "already exists\|Failed to create"; then
            echo "Test user already exists or creation failed: $ERROR_MSG"
        else
            echo "Test user creation issue: $ERROR_MSG"
        fi
    else
        echo "Test user status unknown"
    fi
fi

echo "PocketBase initialization complete. Server running on ${POCKETBASE_URL}"
echo "Admin: ${ADMIN_EMAIL}"
echo "Test User: ${TEST_USER_EMAIL}"

# Wait for PocketBase process to exit
wait $PB_PID