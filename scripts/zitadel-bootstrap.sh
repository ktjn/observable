#!/bin/sh
# scripts/zitadel-bootstrap.sh
#
# Runs once in the zitadel-bootstrap container after Zitadel is healthy.
# Reads the service-account PAT written by Zitadel's first-instance setup,
# creates the Observable OIDC project + app, and writes the client_id to
# /bootstrap/client_id so auth-service can pick it up on start.
set -eu

ZITADEL_BASE="http://zitadel:8080"
PAT_FILE="/bootstrap/admin-pat.txt"
CLIENT_ID_FILE="/bootstrap/client_id"

# Idempotent: skip if a previous run already wrote the client_id.
if [ -f "$CLIENT_ID_FILE" ] && [ -s "$CLIENT_ID_FILE" ]; then
  echo "zitadel-bootstrap: already complete (client_id=$(cat $CLIENT_ID_FILE))"
  exit 0
fi

# Wait for Zitadel to write the PAT file during first-instance setup.
echo "zitadel-bootstrap: waiting for PAT file at $PAT_FILE ..."
i=0
until [ -f "$PAT_FILE" ] && [ -s "$PAT_FILE" ]; do
  i=$((i+1))
  if [ $i -ge 60 ]; then
    echo "zitadel-bootstrap: timed out waiting for PAT file" >&2
    exit 1
  fi
  sleep 2
done
PAT=$(cat "$PAT_FILE")
echo "zitadel-bootstrap: PAT available."

# Wait for Zitadel Management API to be ready.
echo "zitadel-bootstrap: waiting for Zitadel API ..."
i=0
until curl -sf "$ZITADEL_BASE/debug/healthz" > /dev/null 2>&1; do
  i=$((i+1))
  if [ $i -ge 60 ]; then
    echo "zitadel-bootstrap: timed out waiting for Zitadel API" >&2
    exit 1
  fi
  sleep 2
done
echo "zitadel-bootstrap: Zitadel API ready."

# Create the Observable project.
PROJECT_RESPONSE=$(curl -sf -X POST "$ZITADEL_BASE/management/v1/projects" \
  -H "Authorization: Bearer $PAT" \
  -H "Content-Type: application/json" \
  -d '{"name":"Observable"}')
PROJECT_ID=$(echo "$PROJECT_RESPONSE" | jq -r '.id')
echo "zitadel-bootstrap: project id = $PROJECT_ID"

# Create the OIDC web application (public client, PKCE, no secret).
APP_RESPONSE=$(curl -sf -X POST "$ZITADEL_BASE/management/v1/projects/$PROJECT_ID/apps/oidc" \
  -H "Authorization: Bearer $PAT" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Observable Frontend",
    "redirectUris": ["http://localhost/auth/callback"],
    "responseTypes": ["RESPONSE_TYPE_CODE"],
    "grantTypes": ["GRANT_TYPE_AUTHORIZATION_CODE"],
    "appType": "OIDC_APP_TYPE_WEB",
    "authMethodType": "OIDC_AUTH_METHOD_TYPE_NONE",
    "postLogoutRedirectUris": ["http://localhost/login"],
    "devMode": true
  }')
CLIENT_ID=$(echo "$APP_RESPONSE" | jq -r '.clientId')

if [ -z "$CLIENT_ID" ] || [ "$CLIENT_ID" = "null" ]; then
  echo "zitadel-bootstrap: failed to get client_id from response: $APP_RESPONSE" >&2
  exit 1
fi

# Write world-readable so auth-service (uid 65532) can read it.
printf '%s' "$CLIENT_ID" > "$CLIENT_ID_FILE"
chmod 644 "$CLIENT_ID_FILE"

echo "zitadel-bootstrap: done. client_id=$CLIENT_ID"
