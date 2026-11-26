# `CIVBB_state`

Root-level. The document's state table (ADR 0088): typed values that conditions, blend inputs, drivers, menu
controls, contact receivers and chain outputs read and write by index. Names are looked up only at the edges (OSC,
plugins, tools). Schema: [`schema/CIVBB_state.schema.json`](schema/CIVBB_state.schema.json).

## Fields

`entries`, each:

- `name`: unique; `/` separates namespaces. `platform/` is reserved (below).
- `kind`: `bool`, `int`, `float` or `trigger` (a bool that a transition testing it resets).
- `default`: the value an instance starts at; bools and triggers use 0 or 1.
- `range` (optional, int and float only): `{"type": "int", min, max}` or `{"type": "float", min, max, bits}` with
  `bits` 2 to 16. It decides the sync encoding (ADR 0098); absent means unbounded (32 bits when synced).
- `rest` (optional): the value a state with `unwritten: rest` restores where a curve writes this entry, when it
  differs from `default`.
- `sync`: who sends it over the network (`none` or `owner`); `saved`: persisted per user across sessions;
  `external`: inputs from outside the content (menu, OSC, plugins) may write it.

## Rules

- Names are unique and at most 1,024 bytes; `default` and `rest` are finite; a range fits its kind.
- A trigger is never synced.
- The synced entries' bits (bool 1, ranged int ceil(log2(span + 1)), ranged float its `bits`, unranged 32) total at
  most 1,024.
- **Platform values.** An entry named `platform/<name>` (ADR 0096) is provided by the platform every frame. The name
  must be a known platform value; the entry is not synced, saved or external, and no content (driver, curve, menu
  control, contact receiver, chain output) writes it. It may declare a kind other than the platform's; the value is
  converted.
