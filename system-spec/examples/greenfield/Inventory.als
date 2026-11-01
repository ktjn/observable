// Alloy model for `order-stock-projection` and `reserve-inventory` (examples/greenfield/spec.yaml).
// Maps structural invariants to named assertions per .github/skills/alloy/SKILL.md.
module Inventory

sig ProductId, OrderId {}

sig StockItem {
    product_id: one ProductId,
    available_qty: one Int
}

sig Order {
    order_id: one OrderId
}

sig OrderStockProjection {
    order_ref: one Order,
    stock_ref: one StockItem
}

sig State {
    orders: set Order,
    items: set StockItem,
    projections: set OrderStockProjection
}

fact ValidState {
    all s: State | {
        all i: s.items | i.available_qty >= 0
        all p: s.projections | p.order_ref in s.orders and p.stock_ref in s.items
    }
}

pred reserve[s, s': State, o: Order, i, i': StockItem, p: OrderStockProjection, qty: Int] {
    qty > 0
    i in s.items
    i.available_qty >= qty
    i'.product_id = i.product_id
    i'.available_qty = minus[i.available_qty, qty]
    p.order_ref = o
    p.stock_ref = i'
    s'.orders = s.orders + o
    s'.items = (s.items - i) + i'
    s'.projections = s.projections + p
}

// Invariant: stock_never_negative
assert StockNeverNegative {
    all s: State, i: s.items | i.available_qty >= 0
}

// Requirement req-projection: every projection references an existing order and stock-item
assert ProjectionReferencesExisting {
    all s: State, p: s.projections |
        p.order_ref in s.orders and p.stock_ref in s.items
}

// Postcondition: reserve preserves non-negative stock and projection integrity
assert ReservePost {
    all s, s': State, o: Order, i, i': StockItem, p: OrderStockProjection, qty: Int |
        reserve[s, s', o, i, i', p, qty] =>
            (i'.available_qty = minus[i.available_qty, qty] and p.order_ref in s'.orders and p.stock_ref in s'.items)
}

run CanReserve {
    some s, s': State, o: Order, i, i': StockItem, p: OrderStockProjection |
        reserve[s, s', o, i, i', p, 2]
} for 4 but 5 Int

check StockNeverNegative for 5 but 5 Int
check ProjectionReferencesExisting for 5 but 5 Int
check ReservePost for 5 but 5 Int
