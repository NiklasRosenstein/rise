#!/bin/sh
set -e

PB_EXEC="/usr/local/bin/pocketbase"
POCKETBASE_URL="http://localhost:8090"
ADMIN_EMAIL="admin@example.com"
ADMIN_PASSWORD="admin123"
TEST_USER_USERNAME="testuser"
TEST_USER_PASSWORD="test1234"
TEST_USER_EMAIL="testuser@example.com"

# Attempt to create superuser (will exit with 0 even if user exists)
echo "Attempting to create superuser '${ADMIN_EMAIL}'..."
${PB_EXEC} superuser upsert "${ADMIN_EMAIL}" "${ADMIN_PASSWORD}" || echo "Superuser already exists or an error occurred during upsert."

# Authenticate as admin to get a token
ADMIN_AUTH_RESPONSE=$(curl -s -X POST \
  -H "Content-Type: application/json" \
  -d "{\"identity\": \"${ADMIN_EMAIL}\", \"password\": \"${ADMIN_PASSWORD}\"}" \
  "${POCKETBASE_URL}/api/admins/auth-with-password")

ADMIN_TOKEN=$(echo "${ADMIN_AUTH_RESPONSE}" | jq -r '.token')

if [ -z "${ADMIN_TOKEN}" ] || [ "${ADMIN_TOKEN}" = "null" ]; then
  echo "Failed to obtain admin token. Exiting."
  exit 1
fi

echo "Admin token obtained."

# Check if 'users' collection exists
USERS_COLLECTION_ID=$(curl -s -H "Authorization: ${ADMIN_TOKEN}" "${POCKETBASE_URL}/api/collections" | jq -r '.items[] | select(.name == "users") | .id')

if [ -z "${USERS_COLLECTION_ID}" ]; then
  echo "'users' collection not found. Creating it..."
  CREATE_COLLECTION_RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/json" \
    -H "Authorization: ${ADMIN_TOKEN}" \
    -d '{
      "name": "users",
      "schema": [
        {"system":false,"id":"users_name","name":"username","type":"text","required":true,"options":{"min":2,"max":null,"pattern":""}},
        {"system":false,"id":"users_email","name":"email","type":"email","required":true,"options":{"exceptDomains":[],"onlyDomains":[]}},
        {"system":false,"id":"users_password","name":"password","type":"text","required":true,"options":{"min":8,"max":32,"pattern":""}}
      ],
      "listRule": "",
      "viewRule": "",
      "createRule": "",
      "updateRule": "",
      "deleteRule": "",
      "auth": {"allowEmailAuth":true,"allowOAuth2Auth":true,"allowUsernameAuth":true,"exceptDomains":[],"onlyDomains":[]}
    }' \
    "${POCKETBASE_URL}/api/collections")
  echo "Collection creation response: ${CREATE_COLLECTION_RESPONSE}"
  USERS_COLLECTION_ID=$(echo "${CREATE_COLLECTION_RESPONSE}" | jq -r '.id')
  if [ -z "${USERS_COLLECTION_ID}" ]; then
    echo "Failed to create users collection. Exiting."
    exit 1
  fi
else
  echo "'users' collection already exists (ID: ${USERS_COLLECTION_ID})."
fi

# Check if 'testuser' exists
TEST_USER_RECORD=$(curl -s -H "Authorization: ${ADMIN_TOKEN}" "${POCKETBASE_URL}/api/collections/users/records?filter=(username='${TEST_USER_USERNAME}')")
TEST_USER_ID=$(echo "${TEST_USER_RECORD}" | jq -r '.items[0].id')

if [ -z "${TEST_USER_ID}" ] || [ "${TEST_USER_ID}" = "null" ]; then
  echo "Test user '${TEST_USER_USERNAME}' not found. Creating it..."
  CREATE_USER_RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/json" \
    -H "Authorization: ${ADMIN_TOKEN}" \
    -d "{\"username\": \"${TEST_USER_USERNAME}\", \"email\": \"${TEST_USER_EMAIL}\", \"password\": \"${TEST_USER_PASSWORD}\", \"passwordConfirm\": \"${TEST_USER_PASSWORD}\"}" \
    "${POCKETBASE_URL}/api/collections/users/records")
  echo "Test user creation response: ${CREATE_USER_RESPONSE}"
  TEST_USER_ID=$(echo "${CREATE_USER_RESPONSE}" | jq -r '.id')
  if [ -z "${TEST_USER_ID}" ] || [ "${TEST_USER_ID}" = "null" ]; then
    echo "Failed to create test user. Exiting."
    exit 1
  fi
else
  echo "Test user '${TEST_USER_USERNAME}' already exists (ID: ${TEST_USER_ID})."
fi

echo "Initialization complete. Starting PocketBase server."

# Execute the main command to start the server
exec ${PB_EXEC} serve --http=0.0.0.0:8090