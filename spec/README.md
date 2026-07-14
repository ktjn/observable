# Observable Platform Specification

Full-stack observability platform specification — Dynatrace / New Relic class, built on OpenTelemetry.

## Documents

| File | Contents |
|------|----------|
| [00-market-analysis.md](00-market-analysis.md) | Competitive landscape, product positioning, USPs, Tier 2 and Tier 3 feature gaps |
| [01-overview.md](01-overview.md) | Scope, product definition, product principles |
| [02-architecture.md](02-architecture.md) | System context, container diagrams, reference architecture, core technical architecture |
| [03-storage.md](03-storage.md) | Storage strategy, data model, retention tiers, query engine |
| [04-tenancy-security.md](04-tenancy-security.md) | Multi-tenancy models, isolation levels, security specification |
| [05-frontend.md](05-frontend.md) | Frontend stack, modules, UX requirements |
| [06-agents.md](06-agents.md) | Agent and collector strategy |
| [07-alerting-slo.md](07-alerting-slo.md) | Alerting, incident management, SLOs |
| [08-ai-ml.md](08-ai-ml.md) | AI/ML features and constraints |
| [09-api.md](09-api.md) | Public APIs, SDKs, extension points |
| [10-process.md](10-process.md) | ADRs, development process, CI/build process, tiny agent iteration workflow, documentation and spec review process, phased project plan |
| [11-testing.md](11-testing.md) | Test strategy, CI gates, agent iteration verification, no-regression rules, test data, non-functional targets |
| [12-deployment.md](12-deployment.md) | Deployment, build artifacts, tooling recommendations, build-vs-buy |
| [13-risks-roadmap.md](13-risks-roadmap.md) | Risks, initial deliverables, v1 scope, final recommendation |
| [14-domain-model.md](14-domain-model.md) | Data models, entities, and relationships |
| [15-frontend-local-dev.md](15-frontend-local-dev.md) | Frontend local development, storybook, mock data, and developer experience |
| [16-collectable.md](16-collectable.md) | Collectable — now developed in its own repository, [github.com/ktjn/collectable](https://github.com/ktjn/collectable) |
| [17-self-observability.md](17-self-observability.md) | Platform self-observability, monitoring, and health checks |
| [18-deployment-markers.md](18-deployment-markers.md) | Deployment markers and release correlation |
| [19-testbench.md](19-testbench.md) | Test bench: kind-based synthetic workload with full OTel + k8s cluster monitoring |
| [20-nlq-ir-reference.md](20-nlq-ir-reference.md) | NLQ IR canonical reference: DSL grammar, semantic rules, system prompt architecture, metadata injection, shorthand syntax, SQL template library |
| [81-product-lifecycle.md](81-product-lifecycle.md) | Product cost structure (CAPEX/OPEX), lifecycle management, support tiers, versioning and EOL policy |
| [91-customer-tco.md](91-customer-tco.md) | Customer total cost of ownership, acquisition planning, deployment, upgrade strategy, and decommission guide |

## Implementation Plans

Active implementation plans and iteration documents are located in the [docs/superpowers/plans/](../docs/superpowers/plans/) directory.
