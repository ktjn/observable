-- ============================================================================
-- Migration 084: Seed OpenTelemetry Demo catalog services
--
-- Registers the OpenTelemetry Demo microservices under the otel-demo tenant
-- (00000000-0000-0000-0000-000000000004) so Home and Catalog views show
-- demo services immediately alongside ingested telemetry.
-- ============================================================================

INSERT INTO catalog.services (
    tenant_id, business_name, operational_name, domain, description, criticality, lifecycle, classification
) VALUES
    ('00000000-0000-0000-0000-000000000004', 'Frontend', 'frontend', 'storefront', 'OpenTelemetry Demo web storefront', 'critical', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Frontend Proxy', 'frontend-proxy', 'storefront', 'Envoy ingress proxy for OpenTelemetry Demo', 'critical', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Checkout', 'checkout', 'commerce', 'Order checkout orchestrator', 'critical', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Payment', 'payment', 'commerce', 'Payment authorization service', 'critical', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Cart', 'cart', 'commerce', 'Shopping cart service', 'critical', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Product Catalog', 'product-catalog', 'commerce', 'Product catalog lookup service', 'critical', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Accounting', 'accounting', 'finance', 'Order accounting consumer', 'high', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Ad', 'ad', 'storefront', 'Contextual advertisement service', 'standard', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Currency', 'currency', 'commerce', 'Currency conversion service', 'high', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Email', 'email', 'messaging', 'Order confirmation email service', 'standard', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Fraud Detection', 'fraud-detection', 'risk', 'Order fraud screening consumer', 'high', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Quote', 'quote', 'fulfillment', 'Shipping quote calculator', 'standard', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Recommendation', 'recommendation', 'storefront', 'Product recommendation service', 'standard', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Shipping', 'shipping', 'fulfillment', 'Shipping and tracking service', 'high', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Image Provider', 'image-provider', 'storefront', 'Static product image server', 'low', 'live', 'internal'),
    ('00000000-0000-0000-0000-000000000004', 'Load Generator', 'load-generator', 'synthetic', 'Locust synthetic traffic generator', 'low', 'live', 'internal')
ON CONFLICT DO NOTHING;
