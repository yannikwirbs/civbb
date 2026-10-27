# civbb-format

One job: define the content format every other crate agrees on (design doc §9).

- The avatar data model (`Avatar` and its parts) as plain serde types.
- Runtime packages: a CBOR manifest signed with Ed25519, listing BLAKE3-addressed blobs.

No internal dependencies, no engine types. Readers of untrusted packages verify the signature before decoding and every blob hash before use.
