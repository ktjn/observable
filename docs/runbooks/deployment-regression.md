# Deployment Regression Runbook

**Incident class:** deployment regression during canary or rollout

This runbook covers a single failure class: a release that causes unhealthy pods, failed canary gates, or post-promotion SLO regression. The goal is to restore the last known-good release without changing applied migrations.

## When to use this runbook

Use this runbook when any of the following happen:

- `scripts/canary-promote.sh` exits non-zero during canary analysis.
- `helm upgrade` completes, but the new release causes 5xx responses, failing readiness, or broken startup.
- A promoted release regresses service health enough that you want to revert immediately.

## Immediate triage

1. Freeze further promotions for the affected service.
2. Record the current release state:

```bash
helm history observable --namespace observable
helm status observable --namespace observable
```

3. Check whether the canary is still present:

```bash
kubectl get deploy,svc,pod -n observable -l app.kubernetes.io/name=ingest-gateway-canary
```

4. Inspect the failing workload logs:

```bash
kubectl logs -n observable deploy/ingest-gateway
kubectl logs -n observable deploy/ingest-gateway-canary
```

5. Confirm whether the problem is limited to the rollout path or also visible to the stable release:

```bash
kubectl port-forward -n observable service/ingest-gateway 4318:4318
```

```bash
curl -fsS http://localhost:4318/health
```

## Rollback path

### If the canary never promoted

Remove the canary and keep the stable release unchanged:

```bash
helm upgrade observable charts/observable \
  --namespace observable \
  --reuse-values \
  --set services.ingestGateway.canary.enabled=false \
  --wait --timeout 5m
```

### If the new release already promoted

Rollback to the previous Helm revision:

```bash
helm rollback observable <previous-revision> --namespace observable --wait
```

Notes:

- `helm rollback` restores the previous Deployment specs.
- It does not re-run hook Jobs or reverse applied schema migrations.
- Forward-only migrations remain in place by design.

## Restore and verify

After rollback:

1. Wait for the stable deployment to become ready.
2. Re-check health:

```bash
curl -fsS http://localhost:4318/health
```

3. Re-run the relevant canary or smoke gate only after the root cause is understood.
4. If the failure was caused by a bad image tag, keep the bad tag out of the next promotion attempt.

## Escalation criteria

Escalate to the deployment owner if any of the following are true:

- rollback fails;
- pods remain crash-looping after rollback;
- the incident looks like a schema or data issue rather than a release issue;
- the health check recovers, but the customer-visible behavior remains broken.

## References

- [spec/12-deployment.md](../../spec/12-deployment.md)
- [`scripts/canary-promote.sh`](../../scripts/canary-promote.sh)
- [`charts/observable/values.yaml`](../../charts/observable/values.yaml)
