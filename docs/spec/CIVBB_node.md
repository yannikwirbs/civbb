# `CIVBB_node`

On a glTF node. What a glTF node lacks (ADR 0101 point 2). Schema:
[`schema/CIVBB_node.schema.json`](schema/CIVBB_node.schema.json).

## Fields

- `active` (default `true`): whether the node starts active; a clip curve targeting the node's activity changes it.
- `first_person` (default `auto`): first-person visibility of the mesh the node carries:
  - `auto`: visible, except the parts skinned to the head;
  - `both`: visible in both views;
  - `first_person_only`: only in the owner's first-person view;
  - `third_person_only`: hidden from the owner's first-person view.

## Rules

`first_person` appears only on a node carrying a mesh. A node without the extension is active with `auto` visibility.
