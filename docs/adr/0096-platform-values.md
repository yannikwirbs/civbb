# 0096: Platform values: the `platform/` namespace, VRChat's units, lowered built-ins

Status: accepted
Date: 2026-11-26

## Context

ADR 0088 point 3 makes VRChat's 30 built-in parameters (`GestureLeft`, `Upright`, `VelocityX`...) state entries the
platform provides, read-only to content, under native `platform/` names in SI units, with VRChat's names kept in a
table for the importer and OSC (§17). It leaves open how an entry is marked as a platform value, where the table
lives, what happens to the units and declared types of lowered content, and what to do with the content that writes
a built-in (an expression parameter that declares one, a driver that sets one). The corpus (13 exports) declares up
to 15 built-ins per avatar, HAOLAN declares `InStation` as a synced expression parameter, and no driver, menu control
or receiver writes one.

## Decision

1. **The namespace is the marker.** An entry named `platform/<name>` is a platform value; there is no separate
   `source` field. `civbb_format::state::Platform` lists the 30 values with their native names, the type the platform
   provides and VRChat's names (`vrchat_name`, `from_vrchat_name`), so the importer, OSC and the parity hand-off share
   one table. Validation refuses unknown names in the namespace, platform entries that are synced, saved or external,
   and any writer of one: menu controls, contact receivers, chain outputs and driver destinations.
2. **Units stay VRChat's.** VRChat's built-ins are already metres, metres per second and ratios; values are provided
   as VRChat provides them, so lowered thresholds and blend positions need no conversion. `AngularY`'s unit is not
   verified; the locomotion milestone (M1.3) records it and, if it is not SI, the importer converts thresholds then.
   `EyeHeightAsMeters` and `EyeHeightAsPercent` become `platform/eye_height` and `platform/eye_height_percent`.
3. **Declared types hold.** A lowered entry keeps the animator's type (a `GestureLeftWeight` declared Bool stays
   Bool); the platform's value is converted to it, as VRChat does. Defaults and `rest` lower as for any entry
   (ADR 0095); instances still start at `at_rest` values (ADR 0055).
4. **Writers are dropped at import.** The importer renames built-ins into the namespace and clears `sync`, `saved`
   and `external` (VRChat sets them every frame over whatever was synced). Menu controls and receivers resolve
   names after the rename, so a VRChat name references nothing; driver operations whose destination is a platform
   value are removed with a warning.
5. **Edges speak VRChat's names.** The corpus writer raises platform entries to VRChat's names in dialect 1, and the
   parity hand-off to Unity names them as Unity sets them; states coming back resolve a VRChat name to its platform
   value.

## Consequences

- Native content cannot declare its own `platform/` entries; a Unity parameter named `platform/...` that is not a
  platform value fails validation (none in the corpus).
- A synced built-in declared as an expression parameter no longer costs sync bits.
- Packages of format 27 are rebuilt (format 28).
- Revisit point 2 when M1.3 measures `AngularY` and when native content wants SI quantities VRChat lacks.
