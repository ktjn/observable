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

# Internal URL used for API calls (container-to-container).
ZITADEL_BASE="http://zitadel:8080"
# External issuer URL — must match ZITADEL_EXTERNALDOMAIN:ZITADEL_EXTERNALPORT.
ZITADEL_ISSUER="${ZITADEL_ISSUER:-http://localhost:8082}"
# Redirect URI registered in the Zitadel OIDC app — must match what auth-service sends.
REDIRECT_URI="${ZITADEL_REDIRECT_URI:-http://localhost:5173/auth/callback}"

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
until curl -sf -H "Host: localhost" "$ZITADEL_BASE/debug/healthz" > /dev/null 2>&1; do
  i=$((i+1))
  if [ $i -ge 90 ]; then
    echo "zitadel-bootstrap: timed out waiting for API" >&2
    exit 1
  fi
  sleep 2
done
echo "zitadel-bootstrap: API ready."

# Parse key file.
KEY_ID=$(jq -r '.keyId'  "$KEY_FILE")
CLIENT_ID=$(jq -r '.userId' "$KEY_FILE")
jq -r '.key' "$KEY_FILE" > /tmp/sa-private.pem

# Build a JWT (RS256) for the JWT-bearer token exchange.
IAT=$(date +%s)
EXP=$((IAT + 3600))

# base64url-encode a string (no padding, URL-safe chars).
b64url() {
  printf '%s' "$1" | base64 | tr '+/' '-_' | tr -d '=' | tr -d '\n'
}

HEADER=$(b64url "{\"alg\":\"RS256\",\"kid\":\"${KEY_ID}\"}")
PAYLOAD=$(b64url "{\"iss\":\"${CLIENT_ID}\",\"sub\":\"${CLIENT_ID}\",\"aud\":[\"${ZITADEL_ISSUER}\"],\"iat\":${IAT},\"exp\":${EXP}}")

SIGNING_INPUT="${HEADER}.${PAYLOAD}"

SIGNATURE=$(printf '%s' "$SIGNING_INPUT" \
  | openssl dgst -sha256 -sign /tmp/sa-private.pem \
  | base64 | tr '+/' '-_' | tr -d '=' | tr -d '\n')

JWT="${SIGNING_INPUT}.${SIGNATURE}"

# Zitadel uses the Host header to identify the instance (ExternalDomain=localhost).
# All calls go to the internal zitadel:8080 but carry Host: localhost so Zitadel
# can route them to the correct instance.
CURL="curl -sf -H Host:localhost"

# Exchange JWT for an access token.
echo "zitadel-bootstrap: exchanging JWT for access token ..."
TOKEN_RESP=$($CURL -X POST "$ZITADEL_BASE/oauth/v2/token" \
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

# Explicitly set the dev admin password via the Management API.
# The YAML Password field is unreliable — this guarantees the password is set.
ADMIN_PASSWORD="${OBSERVABLE_DEV_ADMIN_PASSWORD:-Dev@Admin1234!}"
# The org domain is derived from org name "Observable" + instance domain "localhost".
USERS_RESP=$($CURL "$ZITADEL_BASE/management/v1/users/_search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"queries":[{"userNameQuery":{"userName":"admin@observable.localhost","method":"TEXT_QUERY_METHOD_EQUALS"}}]}')
ADMIN_USER_ID=$(echo "$USERS_RESP" | jq -r '.result[0].id // empty')
if [ -n "$ADMIN_USER_ID" ]; then
  $CURL -X POST "$ZITADEL_BASE/management/v1/users/$ADMIN_USER_ID/password" \
    -H "Authorization: Bearer $ACCESS_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"password\":\"$ADMIN_PASSWORD\",\"noChangeRequired\":true}" > /dev/null
  echo "zitadel-bootstrap: admin password set for user $ADMIN_USER_ID"
else
  echo "zitadel-bootstrap: admin user not found, skipping password set" >&2
fi

# Create the Observable project.
PROJECT_RESP=$($CURL -X POST "$ZITADEL_BASE/management/v1/projects" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Observable"}')
PROJECT_ID=$(echo "$PROJECT_RESP" | jq -r '.id')
echo "zitadel-bootstrap: project id = $PROJECT_ID"

# Create the OIDC web application (public client, PKCE, no secret).
APP_RESP=$($CURL -X POST "$ZITADEL_BASE/management/v1/projects/$PROJECT_ID/apps/oidc" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Observable Frontend",
    "redirectUris": ["'"$REDIRECT_URI"'"],
    "responseTypes": ["RESPONSE_TYPE_CODE"],
    "grantTypes": ["GRANT_TYPE_AUTHORIZATION_CODE"],
    "appType": "OIDC_APP_TYPE_WEB",
    "authMethodType": "OIDC_AUTH_METHOD_TYPE_NONE",
    "postLogoutRedirectUris": ["'"${REDIRECT_URI%/auth/callback}/login"'"],
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
