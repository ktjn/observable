# Changelog

All notable changes to the `observable-web` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- nginx and the dev proxy route `/v1/tokens` and `/v1/admin/members` to auth-service and `/v1/admin/alerts` to observable-alerting.

### Removed

- The Tenant column on the API tokens page and `tenant_name` on token records.
- The legacy healthy, watch and breach pills, filters and row accents on the telemetry service
  pages, the reliability tab and the home page (phase 1 C4.3). The home banner reports
  business-critical production expectations from the catalog service review, and the
  services list links to it; its highest error rates list is a signal, not a health state.

### Added

- Service catalog search, service records, monitoring coverage, gaps, providers, and ownerless assets.
- Service review (phase 1 C4.2): the catalog service record shows health and agreed
  expectations for one environment class at a time, and the catalog's Service review view
  reports business-critical (or all) services per class from `GET /v1/review/services`.
  Unknown health and expectations are labeled with their reason and never toned as healthy;
  retired services are shown as not live.

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- Traces, logs, metrics, services, infrastructure, alerts, and dashboards views.
- Cross-signal navigation and row drill-downs.
- Natural-language query UI and the in-browser WASM playground.
- OIDC login and tenant-aware API client.
