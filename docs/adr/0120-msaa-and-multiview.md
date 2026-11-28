# 0120: MSAA by render-pass resolve and stereo by multiview in the forward pass

Status: accepted
Date: 2026-11-28

## Context

Design doc §6 core architecture: forward+ with MSAA, both eyes in one pass via multiview. ADR 0023's forward pass
draws one view into the target directly, single-sampled (#60). M1.4 needs the stereo path for the XR swapchain
(#163), whose images are arrays of two layers. Unity's reference renders (ADR 0048) have no MSAA, so parity must
stay single-sampled or the ADR 0084 table moves for a reason that is not shading.

## Decision

1. **MSAA through dynamic rendering's resolve.** With more than one sample the forward pass draws into a transient
   multisampled colour image and a multisampled depth image and resolves the colour (average) into the target at
   the end of the pass; the multisampled colour is not stored. Passes after it (the overlay) draw single-sampled
   into the resolved target. No alpha to coverage yet: masked materials keep their discard.
2. **The sample count is fixed per `Forward`** (pipelines carry it): `forward::Output { color, samples, views }`.
   `Context::samples(n)` gives the highest count up to `n` that both colour and depth framebuffers support. The
   viewer defaults to 4 (`view --msaa N`), `measure` draws as the viewer does with 4; parity always uses 1.
3. **Multiview: one draw per run for both eyes.** A `Forward` built for `views` > 1 has pipelines with that view
   mask and draws into targets with that many layers (`Graph::import_layers`; transients get `ImageDesc::layers`,
   array views). The view buffer holds one record per view; the vertex shader picks its record by `SV_ViewID` and
   hands the index to the fragment shader as a flat varying (eye position for shading). Culling and draw lists are
   shared by the views; a combined stereo frustum is the caller's job (#163). Devices must support `multiview`
   (Vulkan 1.1 core; device selection requires it). The single-view path uses the same shader (view index 0).
4. **The graph learns layers, samples and resolves**: `ImageDesc { samples, layers }`, `Pass::resolve` for the
   first colour attachment, `Pass::views(n)` for the view mask; barriers cover every layer.

## Consequences

- The viewer's GPU time rises with 4x MSAA (measured in STATE.md); `--msaa 1` gives the old picture.
- The overlay stays one view and single-sampled; in VR the menu moves into the world (#170).
- Transient images are still not aliased (ADR 0023); the multisampled colour and depth are the biggest ones now.
- Alpha to coverage for masked materials and a sample-rate choice per headset are open; revisit with #163's
  frame timings.
