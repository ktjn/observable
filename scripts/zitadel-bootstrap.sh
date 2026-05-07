#!/bin/sh
# scripts/zitadel-bootstrap.sh
#
# Runs in the zitadel-bootstrap container after Zitadel has started.
# Reads the RSA machine-key JSON written by Zitadel's first-instance setup,
# uses JWT-bearer auth to get an access token, creates the Observable OIDC
# project + app, and writes the client_id to /bootstrap/client_id so that
# auth-service can pick it up on start.
#
# Requires: curl, jq, openssl  (all available in alpine)
set -eu

ZITADEL_BASE="http://zitadel:8080"
KEY_FILE="/bootstrap/sa-key.json"
CLIENT_ID_FILE="/bootstrap/client_id"

# Idempotent: skip if a previous run already wrote the client_id.
if [ -f "$CLIENT_ID_FILE" ] && [ -s "$CLIENT_ID_FILE" ]; then
  echo "zitadel-bootstrap: already complete (client_id=$(cat $CLIENT_ID_FILE))"
  exit 0
fi

# Wait for Zitadel to write the machine-key file.
echo "zitadel-bootstrap: waiting for machine-key file at $KEY_FILE ..."
i=0
until [ -f "$KEY_FILE" ] && [ -s "$KEY_FILE" ]; do
  i=$((i+1))
  if [ $i -ge 90 ]; then
    echo "zitadel-bootstrap: timed out waiting for key file" >&2
    exit 1
  fi
  sleep 2
done
echo "zitadel-bootstrap: key file found."

# Wait for Zitadel HTTP API to be ready.
echo "zitadel-bootstrap: waiting for Zitadel API ..."
i=0
until curl -sf "$ZITADEL_BASE/debug/healthz" > /dev/null 2>&1; do
  i=$((i+1))
  if [ $i -ge 90 ]; then
    echo "zitadel-bootstrap: timed out waiting for API" >&2
    exit 1
  fi
  sleep 2
done
echo "zitadel-bootstrap: API ready."

# Parse key file.
KEY_ID=$(jq -r '.keyId'   "$KEY_FILE")
CLIENT_ID=$(jq -r '.clientId' "$KEY_FILE")
jq -r '.key' "$KEY_FILE" > /tmp/sa-private.pem

# Build a JWT (RS256) for the JWT-bearer token exchange.
IAT=$(date +%s)
EXP=$((IAT + 3600))

# base64url-encode a string (no padding, URL-safe chars).
b64url() {
  printf '%s' "$1" | base64 | tr '+/' '-_' | tr -d '=' | tr -d '\n'
}

HEADER=$(b64url "{\"alg\":\"RS256\",\"kid\":\"${KEY_ID}\"}")
PAYLOAD=$(b64url "{\"iss\":\"${CLIENT_ID}\",\"sub\":\"${CLIENT_ID}\",\"aud\":[\"${ZITADEL_BASE}\"],\"iat\":${IAT},\"exp\":${EXP}}")

SIGNING_INPUT="${HEADER}.${PAYLOAD}"

SIGNATURE=$(printf '%s' "$SIGNING_INPUT" \
  | openssl dgst -sha256 -sign /tmp/sa-private.pem \
  | base64 | tr '+/' '-_' | tr -d '=' | tr -d '\n')

JWT="${SIGNING_INPUT}.${SIGNATURE}"

# Exchange JWT for an access token.
echo "zitadel-bootstrap: exchanging JWT for access token ..."
TOKEN_RESP=$(curl -sf -X POST "$ZITADEL_BASE/oauth/v2/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data-urlencode "grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer" \
  --data-urlencode "scope=openid urn:zitadel:iam:org:project:id:zitadel:aud" \
  --data-urlencode "assertion=$JWT")

ACCESS_TOKEN=$(echo "$TOKEN_RESP" | jq -r '.access_token')
if [ -z "$ACCESS_TOKEN" ] || [ "$ACCESS_TOKEN" = "null" ]; then
  echo "zitadel-bootstrap: token exchange failed: $TOKEN_RESP" >&2
  exit 1
fi
echo "zitadel-bootstrap: access token obtained."

# Create the Observable project.
PROJECT_RESP=$(curl -sf -X POST "$ZITADEL_BASE/management/v1/projects" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Observable"}')
PROJECT_ID=$(echo "$PROJECT_RESP" | jq -r '.id')
echo "zitadel-bootstrap: project id = $PROJECT_ID"

# Create the OIDC web application (public client, PKCE, no secret).
APP_RESP=$(curl -sf -X POST "$ZITADEL_BASE/management/v1/projects/$PROJECT_ID/apps/oidc" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
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

OIDC_CLIENT_ID=$(echo "$APP_RESP" | jq -r '.clientId')
if [ -z "$OIDC_CLIENT_ID" ] || [ "$OIDC_CLIENT_ID" = "null" ]; then
  echo "zitadel-bootstrap: failed to get clientId: $APP_RESP" >&2
  exit 1
fi

# Write world-readable so auth-service (uid 65532) can read it.
printf '%s' "$OIDC_CLIENT_ID" > "$CLIENT_ID_FILE"
chmod 644 "$CLIENT_ID_FILE"

echo "zitadel-bootstrap: done. client_id=$OIDC_CLIENT_ID"
