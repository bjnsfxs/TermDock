# Architecture (placeholder)

See Plan.md for the full architecture decision record.

High-level:
- Windows host daemon is the single source of truth.
- Clients connect over REST (CRUD/control) + WS (streaming terminal & events).

TODO:
- Add a diagram.
- Define module boundaries inside `daemon/`.
