# 0035: Textures are UASTC or XUASTC within their source's size, with mips shared with the renderer and wrapping atlas gutters

Status: accepted
Date: 2026-11-05

## Context

#65, #68: ADR 0029 shipped colour and data textures as ETC1S (quality 75) and normal maps as UASTC, with Basis
Universal's own mips (Kaiser filter) and atlases with 8-texel edge-repeating gutters on 4-texel boundaries. FLIP
parity (ADR 0033) then put every textured corpus/local file at yellow or red, and KTX2 could be larger than the
source PNG/JPEG. Design doc §9 stage 4 asks for "UASTC for normal maps and high-quality slots, ETC1S for the rest";
it does not say how to trade size against parity. This ADR supersedes the codec, mip and gutter parts of ADR 0029
(its points 3 and 7); the encoder, transcoder, caps, eligibility and stage order stay.

## Measurements

Release, reference PC, all 16 textured files of corpus/local, every compiler stage, parity of the compiled avatar
against its import (worst view and state, mean FLIP; green <= 0.02), and texture bytes against the source images.
Throwaway harness, not committed; numbers from the runs on #65/#68.

- **ETC1S cannot reach green.** At quality 75 it is yellow on every avatar (AliciaSolid 0.061, HAOLAN 0.048, Seed-san
  0.047) and red on AlphaBlendModeTest (stripes turn green) and TextureTransformTest. Quality 100 changes nothing that
  matters (AliciaSolid 0.058): its texel error (1.6 levels mean on AliciaSolid's textures) is the format's ceiling.
- **UASTC (no RDO)** is nearly lossless at level 0 (0.05 levels mean) and turns 13 of 16 files green, but grows
  Booth avatars 1.2 to 1.6x and JPEG sources 2 to 5x. Encoder effort 5 instead of 2 gains 0.001 FLIP for 2.4x time.
  RDO (lambda 0.25 to 2) saves 10 to 20 % and costs more FLIP than XUASTC does for the same bytes.
- **XUASTC LDR 4x4** (Basis Universal 2.0, transcoded by the same `basisu` crate) at quality 100 is 10 % smaller
  than UASTC with half the texel error; quality 75 is 0.55 to 0.94x the source on the Booth avatars and still green
  there (HAOLAN 0.017). On desktop it loses a little in the ASTC-to-BC7 transcode, so UASTC scores best per byte
  when it fits.
- **Per kind** (all corpus textures, bytes against source, mean level-0 error in 8-bit levels):

  | kind / source | UASTC | XUASTC 100 | XUASTC 75 | XUASTC 50 |
  | --- | --- | --- | --- | --- |
  | colour, PNG (45.7 MB) | 1.23x, 0.07 | 1.12x, 0.03 | 0.71x, 0.14 | 0.62x, 0.18 |
  | colour, JPEG (0.9 MB) | 4.97x, 0.32 | 4.34x, 0.23 | 2.63x, 0.69 | 2.33x, 0.87 |
  | normal, PNG (4.4 MB) | 0.89x, 0.20 | 0.73x, 0.14 | 0.41x, 0.38 | 0.34x, 0.44 |
  | normal, JPEG (1.2 MB) | 2.62x, 1.25 | 2.23x, 0.98 | 1.58x, 1.76 | 1.33x, 2.06 |
  | data, PNG (3.2 MB) | 0.81x, 0.01 | 0.81x, 0.01 | 0.35x, 0.18 | 0.25x, 0.20 |
  | data, JPEG (1.0 MB) | 3.67x, 0.48 | 3.11x, 0.38 | 1.96x, 0.81 | 1.67x, 0.95 |

  The kinds do not need different codecs: PNG normal and data maps fit as UASTC anyway, PNG colour needs XUASTC to
  fit, and nothing fits a JPEG at a usable quality.
- **Mips.** With UASTC the remaining error is not level 0 (UASTC's level 0 fed back as PNG scores 0.005 on
  AliciaSolid) but the small mip levels a 640-pixel full-body view samples: UASTC error grows from 0.05 levels at
  level 0 to 0.3 to 1.5 at levels 2 to 5; the BC7 transcode adds about 0.004 FLIP. Basis Universal's Kaiser mips
  against the renderer's blitted mips added more; offline mips uploaded as RGBA8 still scored 0.006 to 0.017 against
  the GPU blit, so the two paths can only agree if they share one mip definition.
- **Atlas.** Before compression, the atlas alone cost AliciaSolid 0.0116 (8-texel clamped gutters, 4-texel
  alignment, odd atlas sides, so mips mixed neighbours and drifted). With the gutters and alignment below and shared
  mips it costs 0.0017 (twist sample 0.0006).

## Decision

1. **A codec ladder per texture, within the source's size.** Each texture is encoded with UASTC LDR 4x4 (effort 2,
   no RDO, Zstandard), then XUASTC LDR 4x4 at quality 90, then 75, and the first whose KTX2 is no larger than the
   texture's source blob is kept; when none fits (JPEG sources), the smallest of the three is kept and counted as
   `larger` in the stage's log. Every kind uses the same ladder (the measurements above show it picks well per
   kind). ETC1S is no longer produced; the client still transcodes it.
2. **One mip definition.** `civbb_core::mips::mips` makes every mip chain on the CPU: each level half the one above
   (rounded down, at least 1), the bilinear sample at each texel centre with clamped edges (a 2x2 box for even
   sides), in linear light for sRGB colour, alpha linear, 8-bit per level. The compiler hands these levels to Basis
   Universal instead of letting it generate its own, and the renderer uploads RGBA8 textures (unstaged imports, the
   parity reference) with the same levels instead of blitting on the GPU. Source and compiled now differ only by the
   codec.
3. **Atlas gutters wrap, 16 texels, 16-aligned.** Each rectangle's gutter repeats its texture periodically, as the
   repeating sampler sees the separate texture; rectangles and atlas sides sit on 16-texel boundaries, so mips 0 to 4
   of a rectangle are exactly its texture's own (for sides that are multiples of 16). Atlases are written as PNG at
   the best compression level, so an atlas's size budget for the ladder tracks the textures it replaces.
4. **At most four encoder threads.** One 4096-texel encode needs about 3 GB; a thread per core (16 on the reference
   PC) committed 51 GB in a test run.
5. **Platforms.** The same ladder for Quest, after its 2048 cap. XUASTC is ASTC, which Quest samples natively when
   the mobile path (ASTC transcode) lands; desktop transcodes both codecs to BC7.

## Consequences

- Parity after this change (worst mean; bytes against source): HAOLAN 0.0141 (0.80x), HAOLAN_Lowpoly 0.0135 (0.80x),
  Taro 0.0134 (1.00x), TessakuChan 0.0153 (0.84x), Seed-san 0.0181 (0.80x), twist sample 0.0141 (0.74x), and 9 of
  the Khronos samples green; textured files green 13 of 16 (before: 4 of 16, of which 2 red).
- Accepted yellow: AliciaSolid 0.0249 at 0.77x (its atlas does not fit as UASTC and goes to XUASTC; UASTC
  everywhere would be 0.021, still yellow, at 0.94x), Avocado 0.026 and AlphaBlendModeTest 0.045 (fine stripes and
  cutout edges at small mips; no codec in the study reaches green on them, UASTC included). Revisit the green bound
  when Unity references exist (ADR 0033): Unity against us adds its own noise.
- JPEG sources grow (AlphaBlendModeTest 2.0x, Fox 2.5x, the tiny Khronos samples up to 5x). Only Khronos samples
  have JPEGs in corpus/local; `larger` in the log and the report's stage bytes show it. A lower ladder rung for them
  would trade visible quality for bytes JPEG already threw away; not done.
- Builds are slower: up to three encodes per texture (HAOLAN 90 s, Taro 131 s, AliciaSolid 80 s with its atlas).
  XUASTC adds a transcode path the client must handle: the damaged-file test found `basisu` 0.1 multiplying the
  header's level, layer and face counts unchecked on it, so the client now rejects anything but one 2D image
  (depth 0, one layer, one face) before calling it.
- Loading an RGBA8 texture now builds its mips on the CPU (only unstaged imports and parity references use RGBA8).
