# 0149 — Progressive JPEG decode

Status: completed 2026-08-20
Owner: engine (gfx/jpeg)
Effort: M

## The field ask

Operator, 2026-08-20, from the gateway console's artifact inspector: a
photo preview showed `cannot decode this image here: parse: jpeg:
progressive JPEG not supported (baseline only)` instead of the picture.

Progressive is not an exotic corner of the format. It is what most
editors emit by default, what "save for web" pipelines produce, and what
photo libraries and phone shares carry. A preview pane that refuses it
refuses a large share of the real files users point at it.

## Shipped

- `gfx::jpeg` decodes SOF2 (progressive) beside SOF0/SOF1, over the same
  frame geometry, sampling factors, restart markers, and guards.
- Every scan decodes into a per-component COEFFICIENT plane (`i16`,
  zigzag order) that survives across scans; dequantize + IDCT run once
  per block after the last scan, component by component, so each
  coefficient buffer is released as its samples appear. Sequential
  decoding is the one-scan case of the same walk — one code path.
- `gfx::jpeg_entropy` gained the four progressive block decoders
  (T.81 G.2 DC first/refine, AC first/refine): spectral selection,
  successive approximation, and EOB runs as scan state, with restart
  markers resetting the DC predictors and the EOB run together.
- Single-component (non-interleaved) scans came with it, so multi-scan
  SEQUENTIAL files decode too — those used to reject by name.
- New named rejections replace the old blanket ones: `Ah != Al + 1`, a
  progressive AC scan with more than one component, a progressive DC
  scan with `Se != 0`, and any component the scans never mention (an
  all-zero plane must never assemble silently). Arithmetic, lossless,
  hierarchical, 12-bit, and CMYK still reject by name.

## Evidence

- Fixtures are real `cjpeg` output with the regeneration commands
  embedded: progressive 4:4:4, 4:2:0, 4:2:0 + restarts, progressive
  grayscale, and a `-scans` sequential non-interleaved file. The 4:4:4
  progressive fixture is the twin of the sequential one, and the two
  decodes are pinned against each other.
- The truncation ladder and the marker-soup fuzz cover the progressive
  path; the 3k-mutation big fuzz stays panic-free.
- Field check: the 1200x1599 progressive photo from the report decodes
  to a mean absolute error of 0.16/255 per channel against the platform
  decoder (max 10, from nearest-neighbour chroma upsampling).

## Limits kept

Chroma upsampling stays NEAREST — a smooth upsampler remains a measured
decision, not a default cost, and at terminal resolutions the difference
is the max-10 tail above. Arithmetic coding, lossless, hierarchical,
12-bit precision, and CMYK still reject by name.
