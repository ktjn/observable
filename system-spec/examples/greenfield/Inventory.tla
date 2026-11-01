--------------------------- MODULE Inventory ---------------------------
\* TLA+ model for capability `reserve-inventory` (examples/greenfield/spec.yaml).
\* Maps contract clauses to named properties per .github/skills/tla/SKILL.md.
EXTENDS Naturals

CONSTANTS Products, MaxStock, MaxQty

VARIABLES stock, reserved, outcome

vars == <<stock, reserved, outcome>>

Init ==
    /\ stock \in [Products -> 0..MaxStock]
    /\ reserved = [p \in Products |-> 0]
    /\ outcome = "idle"

\* Preconditions: product_exists (p \in Products), quantity_positive (q > 0)
\* Postconditions: reservation_created, stock_decremented
Reserve(p, q) ==
    /\ p \in Products
    /\ q \in 1..MaxQty
    /\ stock[p] >= q
    /\ stock' = [stock EXCEPT ![p] = @ - q]
    /\ reserved' = [reserved EXCEPT ![p] = @ + q]
    /\ outcome' = "reservation_created"

\* Failure mode: insufficient_inventory
InsufficientInventory(p, q) ==
    /\ p \in Products
    /\ q \in 1..MaxQty
    /\ stock[p] < q
    /\ UNCHANGED <<stock, reserved>>
    /\ outcome' = "insufficient_inventory"

Next ==
    \E p \in Products, q \in 1..MaxQty :
        Reserve(p, q) \/ InsufficientInventory(p, q)

Spec == Init /\ [][Next]_vars

\* Invariant: stock_never_negative
StockNeverNegative ==
    \A p \in Products : stock[p] >= 0

\* Conservation across reservations and remaining stock
StockConserved ==
    \A p \in Products : stock[p] + reserved[p] <= MaxStock

=============================================================================
