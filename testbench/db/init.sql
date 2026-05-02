CREATE TABLE IF NOT EXISTS products (
  id    SERIAL PRIMARY KEY,
  name  TEXT NOT NULL,
  price NUMERIC(10,2) NOT NULL
);
CREATE TABLE IF NOT EXISTS users (
  id    SERIAL PRIMARY KEY,
  name  TEXT NOT NULL,
  email TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS orders (
  id         SERIAL PRIMARY KEY,
  user_id    INTEGER NOT NULL REFERENCES users(id),
  product_id INTEGER NOT NULL REFERENCES products(id),
  total      NUMERIC(10,2) NOT NULL,
  status     TEXT NOT NULL DEFAULT 'pending',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS inventory (
  product_id INTEGER PRIMARY KEY REFERENCES products(id),
  quantity   INTEGER NOT NULL DEFAULT 0 CHECK (quantity >= 0),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO products (name, price) VALUES
  ('Widget A', 9.99), ('Widget B', 14.99), ('Gadget X', 49.99),
  ('Gadget Y', 79.99), ('Thingamajig', 4.99),
  ('Doohickey', 24.99), ('Whatchamacallit', 39.99),
  ('Gizmo Pro', 99.99), ('Contraption', 19.99), ('Doodad', 7.99)
ON CONFLICT DO NOTHING;

INSERT INTO users (name, email) VALUES
  ('Alice', 'alice@testbench.local'),
  ('Bob', 'bob@testbench.local'),
  ('Carol', 'carol@testbench.local'),
  ('Dave', 'dave@testbench.local'),
  ('Eve', 'eve@testbench.local')
ON CONFLICT DO NOTHING;

INSERT INTO inventory (product_id, quantity)
SELECT id, 50 FROM products
ON CONFLICT DO NOTHING;
