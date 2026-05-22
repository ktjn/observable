# Operational Runbooks

This directory contains operator-facing runbooks for production failure classes.

Current coverage:

- [Deployment regression runbook](deployment-regression.md) - canary or rollout failures where the safest recovery is to roll back to the last known-good release and verify health before retrying promotion.
