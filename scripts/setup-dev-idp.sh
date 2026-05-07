#!/usr/bin/env bash
# scripts/setup-dev-idp.sh
# One-time Zitadel configuration for local dev.
# Run after: docker compose up zitadel --wait
#
# Outputs: ZITADEL_CLIENT_ID written to .env.local
set -euo pipefail

ZITADEL_BASE="${ZITADEL_BASE:-http://localhost:8082}"
ADMIN_PASSWORD="${OBSERVABLE_DEV_ADMIN_PASSWORD:-Dev@Admin1234!}"

echo "Waiting for Zitadel to be ready..."
for i in $(seq 1 30); do
  if curl -sf "${ZITADEL_BASE}/debug/healthz" > /dev/null 2>&1; then
    echo "Zitadel is healthy."
    break
  fi
  echo "  attempt $i/30..."
  sleep 2
done

# 1. Obtain a token by logging in as the instance admin.
TOKEN=$(curl -sf -X POST "${ZITADEL_BASE}/oauth/v2/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data-urlencode "grant_type=password" \
  --data-urlencode "username=admin@dev.observable" \
  --data-urlencode "password=${ADMIN_PASSWORD}" \
  --data-urlencode "scope=openid profile email urn:zitadel:iam:org:project:id:zitadel:aud" \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['access_token'])")

echo "Obtained admin token."

# 2. Get the default org ID.
DEFAULT_ORG_ID=$(curl -sf "${ZITADEL_BASE}/management/v1/orgs/me" \
  -H "Authorization: Bearer ${TOKEN}" \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['org']['id'])")
echo "Default org ID: ${DEFAULT_ORG_ID}"

# 3. Create an Observable project.
PROJECT_ID=$(curl -sf -X POST "${ZITADEL_BASE}/management/v1/projects" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"name":"Observable"}' \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
echo "Project ID: ${PROJECT_ID}"

# 4. Create a public PKCE web OIDC app (no client secret needed for public clients).
APP_RESPONSE=$(curl -sf -X POST "${ZITADEL_BASE}/management/v1/projects/${PROJECT_ID}/apps/oidc" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Observable Frontend",
    "redirectUris": ["http://localhost:5173/auth/callback"],
    "responseTypes": ["RESPONSE_TYPE_CODE"],
    "grantTypes": ["GRANT_TYPE_AUTHORIZATION_CODE"],
    "appType": "OIDC_APP_TYPE_WEB",
    "authMethodType": "OIDC_AUTH_METHOD_TYPE_NONE",
    "postLogoutRedirectUris": ["http://localhost:5173/login"],
    "devMode": true
  }')

CLIENT_ID=$(echo "${APP_RESPONSE}" | python3 -c "import json,sys; print(json.load(sys.stdin)['clientId'])")
echo "OIDC Client ID: ${CLIENT_ID}"

# 5. Write to .env.local for auth-service to pick up.
cat > .env.local <<EOF
ZITADEL_ISSUER=http://localhost:8082
ZITADEL_CLIENT_ID=${CLIENT_ID}
ZITADEL_REDIRECT_URI=http://localhost:5173/auth/callback
SESSION_SECRET=$(openssl rand -hex 32 2>/dev/null || python3 -c "import secrets; print(secrets.token_hex(32))")
EOF

echo ""
echo "Done! Written to .env.local:"
cat .env.local
echo ""
echo "Restart auth-service to pick up the new config:"
echo "  docker compose up --build auth-service"
