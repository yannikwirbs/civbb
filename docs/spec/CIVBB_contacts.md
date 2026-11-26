# `CIVBB_contacts`

Root-level. Contact senders and receivers (ADR 0047): senders emit tags, receivers detect overlapping senders with a
matching tag and write a state entry. Schema: [`schema/CIVBB_contacts.schema.json`](schema/CIVBB_contacts.schema.json).

## Fields

- A volume (both lists): `node` it is placed on; optional `component`, the node whose activity enables the contact
  when it is not `node`; `shape`: `sphere` (`radius`), `capsule` (`radius`, `height`) or `box` (`size`); `position`
  and `rotation` relative to the node.
- `senders`: `volume`, `tags`, `local_only` (exists only on the owner's client).
- `receivers`: `volume`, `tags` (any match counts), optional `parameter` (the entry written; absent writes nothing),
  `kind`, `allow_self`, `allow_others`, `local_only`, `min_velocity` (metres per second, for `on_enter`) and `face`
  (box receivers: proximity towards the +Z face instead of the centre). Kinds:
  - `constant`: 1 while any sender overlaps;
  - `on_enter`: 1 for one frame when a sender enters;
  - `proximity`: 0 to 1 by how deep the nearest sender is.

## Rules

Volume and component nodes exist; sizes are finite and 0 or more; positions and rotations are finite; tags are at most
1,024 bytes; a receiver's parameter is an existing entry and not a platform value. At most 1,024 senders and
receivers together.
