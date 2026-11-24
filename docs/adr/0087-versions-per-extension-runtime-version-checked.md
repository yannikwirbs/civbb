# 0087: Each native extension has its own version and migrations; runtime packages are checked and rebuilt

Status: accepted
Date: 2026-11-24

## Context

#100 goal 5: features can be added without re-versioning everything, and every older source stays readable, tested.
Today one `FORMAT_VERSION` (25) covers the whole model, is written into runtime packages and never checked on read,
and ADR 0041 point 5 promises migrations that do not exist. Design doc §9 keeps sources and recompiles them; the
manifest records the compiler version.

Options:

1. One global version with a chain of whole-model migrations. Every field change in one part bumps all parts.
2. An integer version per extension, a migration per breaking step on the JSON, a frozen fixture per version.
3. Semantic versions per extension (minor additive, as WIT packages, §11). The minor tells our readers nothing,
   since the platform recompiles everything with the newest pipeline.
4. No versions, additive changes only, a new extension name for each break. Fragments the spec.

## Decision

Option 2 for sources; runtime packages are caches.

1. **Source extensions.** Every native extension object carries `"version": N` (from 1). Adding an optional field
   whose default keeps the old behaviour does not bump it. Anything else bumps it and adds a migration from N-1 to
   N in `civbb_format::migrate`, run on the JSON value before deserialising. Readers accept every version up to
   their own and refuse newer ones by name.
2. **Fixtures.** Each version of each extension keeps a frozen fixture (`crates/format/tests/fixtures/<extension>/
   v<N>.json`) that must migrate, deserialise and validate forever.
3. **Unity dialect** (ADR 0085): versioned as a whole, since the exporter writes every part at once, by
   `asset.extras.civbb_unity_dialect` (absent means 1); its frozen fixtures play the same role.
4. **Runtime packages are not migrated.** `Header::format_version` versions the compiled CBOR; it bumps on any change
   to it, and a reader refuses any other version with a typed error. The service recompiles kept sources when it
   bumps (§9), so a client never needs an old reader.

## Consequences

- One part can change without touching the others; the cost of a breaking change is one migration and one
  fixture, paid when it is made.
- Old content can never silently misread: an old runtime package fails loudly and is rebuilt.
- Extension versions start at 1 when the native extensions are written (after ADRs 0088 to 0090 settle); until then
  the native model has no readers but ours. The runtime check comes first (it guards existing packages).
