ALTER TABLE api_keys
    ADD COLUMN role TEXT NOT NULL DEFAULT 'member'
        CHECK (role IN ('viewer', 'member', 'admin'));

-- Seed a viewer-role dev key: value = "dev-viewer-key-0000"
-- SHA-256("dev-viewer-key-0000") = 5cc0e90452dcc39f43c2b3d95048ef0067ccd862cb5c61e2e12cce10bdd53d8e
INSERT INTO api_keys (tenant_id, key_hash, name, role) VALUES (
    '00000000-0000-0000-0000-000000000001',
    '5cc0e90452dcc39f43c2b3d95048ef0067ccd862cb5c61e2e12cce10bdd53d8e',
    'dev-viewer-key',
    'viewer'
) ON CONFLICT DO NOTHING;
