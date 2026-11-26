# 0098: The sync encoding of a state entry: ranges, unranged values and the exact midpoint

Status: accepted
Date: 2026-11-26

## Context

ADR 0088 point 4 derives each synced entry's wire encoding from its type: bool 1 bit, int with a range
ceil(log2(span + 1)) bits, float with a range `bits` (default 8) quantised uniformly, triggers unsynced; budgets of
256 bits for lowered legacy content and 1,024 for native content. It leaves open what an unranged synced int or
float costs, how "uniformly" places the codes, which `bits` are allowed, and how the two budgets are told apart.
Until #118 every int and float cost 8 bits whatever it was. The 13 Unity exports have no synced parameter whose
animator type differs from its expression type.

## Decision

1. `Entry::range: Option<Range>`, `Range::Int { min, max }` (i32, `min <= max`) or `Range::Float { min, max, bits }`
   (finite, `min < max`, `bits` 2 to 16). A range belongs to the entry's own type; bools and triggers have none.
2. Costs (`Entry::sync_bits`): bool 1; ranged int ceil(log2(max - min + 1)) (0 when `min == max`); ranged float
   `bits`; **unranged int or float 32**, the value's full width, so leaving out a range is allowed but costs what
   it carries. Triggers are never synced: validation refuses a trigger with `sync: owner`.
3. A ranged float's codes are `0 ..= 2^bits - 2`, i.e. `2^bits - 2` equal steps from `min` to `max`, so `min`, the
   midpoint and `max` are exact: -1..1 in 8 bits is a step of 1/127 and carries 0, -1 and 1 exactly, as lowered
   floats need (menus rest at 0). One code of the `2^bits` is unused. Values outside the range clamp; a ranged int rounds to the
   nearest integer and clamps. `Entry::encode` and `Entry::decode` are the one definition the protocol (M1.5)
   uses.
4. Lowering (Unity dialect 1): an expression parameter's entry gets `Int 0..255` (8 bits) or `Float -1..1, 8 bits`
   by the entry's type; animator-only parameters stay unranged and unsynced. VRM expressions are `Float 0..1, 8
   bits`.
5. `validate` enforces one budget, `Caps::synced_bits` (1,024). The 256-bit legacy budget is not checked again:
   lowered VRChat content is already within it by its source's own rule, and the importer does not raise costs
   (every lowered type costs what VRChat charged).

## Consequences

- Corpus synced-bit totals are unchanged by construction; the report's `synced bits` row now reads from
  `Entry::sync_bits`.
- A native creator who wants a cheap counter must give it a range; the 32-bit fallback is visible in the report.
- Format v29: `Entry` gained a field.
