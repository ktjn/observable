# Identity and Authorization Runbook

**Incident class:** Zitadel OIDC token exchange failure, session validation rejection, or OpenFGA authorization outage

This runbook guides operators through diagnosing and resolving authentication and authorization failures affecting the web UI, API keys, and internal service calls.

## When to use this runbook

Use this runbook when:
- Users cannot log into the frontend UI or get redirected in an infinite login loop.
- `auth-service` returns HTTP 500 on `/v1/oauth/callback` or `/internal/validate-session`.
- `admin-service` or `query-api` report `OpenFGA check failed` or HTTP 403 on valid user operations.
- API keys return HTTP 401 across all ingest or query operations.

## Architecture Context

```
Browser User
     |
     v
[Frontend SPA]  ----(OIDC / OAuth 2.0)---->  [Zitadel IdP] (:8082)
     |
     v (session cookie)
[query-api / admin-service]  --(/internal/validate-session)--> [auth-service] (:4319)
     |                                                              |
     +----(check relation: tenant_editor/viewer)--------------------+
     |                                                              |
     v                                                              v
 [OpenFGA] (:8083)                                         PostgreSQL (`auth` schema)
```

1. Browser signs in via Zitadel OIDC; `auth-service` exchanges the authorization code and creates a signed session cookie.
2. Cross-tenant authorization delegates to `auth-service` via internal HTTP (`POST /internal/validate-session`).
3. Fine-grained resource checks (e.g. dashboard edit, alert mute) query OpenFGA.
4. Database state is partitioned into isolated PostgreSQL schema `auth` with login role `observable_auth`.

## Immediate Triage

1. **Check auth-service pod status and logs**:
   ```bash
   kubectl get pods -n observable -l app.kubernetes.io/name=auth-service
   kubectl logs -n observable deploy/auth-service --tail=100
   ```

2. **Check Zitadel readiness**:
   ```bash
   kubectl port-forward -n observable service/zitadel 8082:8080 &
   PF_PID=$!
   curl -fsS http://localhost:8082/debug/ready
   kill $PF_PID
   ```

3. **Check OpenFGA readiness**:
   ```bash
   kubectl port-forward -n observable service/openfga 8083:8081 &
   PF_PID=$!
   curl -fsS http://localhost:8083/healthz
   kill $PF_PID
   ```

4. **Verify PostgreSQL connectivity for auth**:
   ```bash
   kubectl exec -n observable -it deploy/auth-service -- curl -fsS http://localhost:4319/health
   ```

## Mitigation & Recovery Paths

### Scenario A: OIDC Token Exchange Fails (Callback Error)

**Symptom**: `auth-service` logs `failed to exchange OIDC code: invalid_grant` or `redirect_uri mismatch`.

1. Verify `ZITADEL_REDIRECT_URI` matches the ingress/frontend public URL:
   ```bash
   kubectl exec -n observable -it deploy/auth-service -- env | grep -E "ZITADEL_|SESSION_"
   ```
2. Verify system clock synchronization between nodes (clock skew >5s rejects OIDC tokens):
   ```bash
   kubectl get nodes -o wide
   ```
3. If client secret or client ID was regenerated in Zitadel:
   Update the secret in Helm `values.yaml` and rollout restart:
   ```bash
   kubectl rollout restart -n observable deploy/auth-service
   ```

### Scenario B: OpenFGA Authorization Check Failures

**Symptom**: Valid admin operations return HTTP 500 / 403; logs show `OpenFGA connection refused` or `model not found`.

1. Verify OpenFGA store initialization:
   ```bash
   kubectl logs -n observable -l app.kubernetes.io/name=openfga-bootstrap
   ```
2. If store ID is missing or invalid:
   Check `OPENFGA_STORE_ID` environment variable on `admin-service` and `auth-service`.
3. If OpenFGA service restarted and lost transient state, re-run model bootstrapping job:
   ```bash
   kubectl create job --from=cronjob/openfga-bootstrap openfga-bootstrap-manual -n observable
   ```

### Scenario C: Session Secret Invalidation / Rotation

**Symptom**: All logged-in users receive HTTP 401 after release or pod restart.

1. Cause: `SESSION_SECRET` was not pinned in Helm values and auto-generated randomly on pod restart.
2. Fix: Pin a permanent 32-byte base64 secret in Helm values or Kubernetes Secret:
   ```yaml
   identity:
     sessionSecret: "FIXED_LONG_RANDOM_SECRET_BASE64"
   ```
3. Apply via `helm upgrade` so future restarts preserve user sessions.

## Restore and Verify

1. Test internal session validation endpoint:
   ```bash
   kubectl exec -n observable -it deploy/query-api -- curl -s -X POST \
     http://auth-service:4319/internal/validate-session \
     -H "Content-Type: application/json" \
     -H "Authorization: Bearer $INTERNAL_SERVICE_TOKEN" \
     -d '{"session_token":"nonexistent"}' | jq .
   ```
2. Assert JSON response returns `{"valid":false}` rather than connection error or HTTP 500.
3. Test browser login flow at `http://localhost:5173`.

## Escalation Criteria

Escalate to security/platform team if:
- API key cryptographic signing secret or masterkey is compromised.
- OpenFGA authorization model contains circular references causing stack overflow.
- PostgreSQL database schema `auth` has corrupted user records or foreign keys.
