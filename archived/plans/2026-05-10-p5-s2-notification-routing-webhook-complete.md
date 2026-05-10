# P5-S2: Notification Routing Integration (Webhook)

**Goal:** Implement rule-level notification channels starting with Webhook, including retry and audit behavior.

---

## 1. Context & Motivation

Currently, the platform evaluates alert rules and records firings in the database, but it does not notify operators. This slice adds the foundational notification routing model specified in `spec/07-alerting-slo.md §11.4`, focusing on rule-level webhook integrations.

---

## 2. Design

### 2.1 Data Model

We introduce two new tables for notification management and one audit table for tracking deliveries.

**Table: `notification_channels`**
- `channel_id` (UUID, PK)
- `tenant_id` (UUID, FK)
- `name` (TEXT)
- `type` (TEXT, e.g., 'webhook')
- `config` (JSONB, stores URL, headers, etc.)
- `created_at` (TIMESTAMPTZ)
- `updated_at` (TIMESTAMPTZ)

**Table: `notification_audit_log`**
- `audit_id` (UUID, PK)
- `tenant_id` (UUID)
- `firing_id` (UUID, FK to `alert_firings`)
- `channel_id` (UUID, FK to `notification_channels`)
- `state` (TEXT: 'pending', 'sent', 'failed')
- `error_message` (TEXT, optional)
- `retry_count` (INT, default 0)
- `last_attempt_at` (TIMESTAMPTZ)
- `created_at` (TIMESTAMPTZ)

**Table Update: `alert_rules`**
- Add `notification_channels` (UUID[], optional) to store rule-level channel associations.

### 2.2 Notification Dispatcher

The `alert-evaluator` service will gain a new worker: `notification_worker`.

1. **Enqueuing:** When `record_firing` or `resolve_open_firing` transitions an alert to `active` or `resolved`, it will insert records into `notification_audit_log` for each channel associated with the rule.
2. **Dispatching:** The `notification_worker` polls `notification_audit_log` for `pending` records.
3. **Webhook Execution:** For each record, it fetches the channel config and sends a POST request with the alert detail.
4. **Retry Logic:** On failure, it increments `retry_count` and updates `last_attempt_at`. It will retry with exponential backoff (e.g., 10s, 1m, 5m, 1h) up to a max of 10 retries.

### 2.3 Webhook Payload Format

```json
{
  "version": "1",
  "firing_id": "...",
  "rule_id": "...",
  "rule_name": "High Error Rate",
  "tenant_id": "...",
  "severity": "critical",
  "state": "active", // or "resolved"
  "value": 0.08,
  "occurred_at": "2026-05-10T14:00:00Z"
}
```

---

## 3. Implementation Plan

### Phase 1: Database Migrations

- [ ] Create `migrations/postgres/024_create_notification_channels.sql`
- [ ] Create `migrations/postgres/025_add_channels_to_alert_rules.sql`

### Phase 2: Backend (query-api)

- [ ] Update `services/query-api/src/alerts.rs`:
  - Update `AlertRuleItem` and `CreateRuleRequest` to include `notification_channels`.
  - Update `create_alert_rule` to persist the channels.
- [ ] Create `services/query-api/src/notifications.rs`:
  - Implement CRUD for notification channels.
- [ ] Register notification routes in `services/query-api/src/main.rs`.

### Phase 3: Backend (alert-evaluator)

- [ ] Update `services/alert-evaluator/src/evaluator.rs`:
  - Update `AlertRuleRow` and queries to fetch `notification_channels`.
  - Update `record_firing` and `resolve_open_firing` to insert into `notification_audit_log`.
  - Implement `notification_worker` with retry logic and `reqwest` dispatcher.
- [ ] Update `services/alert-evaluator/src/main.rs` to start the `notification_worker`.

### Phase 4: Frontend

- [ ] Update `apps/frontend/src/api/alerts.ts` types and calls.
- [ ] Create `apps/frontend/src/api/notifications.ts`.
- [ ] Create `apps/frontend/src/features/alerts/NotificationChannelsPage.tsx`.
- [ ] Update `apps/frontend/src/features/alerts/AlertsPage.tsx` form.

---

## 4. Verification Plan

### Automated Tests
- [ ] **Rust Unit Tests:** Test webhook payload generation and retry backoff logic in `alert-evaluator`.
- [ ] **Postgres Integration Tests:** Verify `notification_audit_log` enqueuing on firing transitions.
- [ ] **HTTP Integration Tests:** Verify `query-api` notification channel CRUD endpoints.

### Manual Verification
1. Start the platform with `docker compose up -d`.
2. Create a webhook notification channel pointing to a mock listener (e.g., `http://httpbin.org/post` or a local listener).
3. Create an alert rule with that channel.
4. Trigger the alert (e.g., by emitting a metric that exceeds the threshold).
5. Verify the webhook is received and `notification_audit_log` shows `sent`.

---

## 5. Rollback Plan

1. Revert code changes in `alert-evaluator` and `query-api`.
2. Run `sqlx migrate revert` to remove notification tables and columns.
