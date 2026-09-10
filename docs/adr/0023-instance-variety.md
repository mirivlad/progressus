# ADR-0023 — Instance variety in procedural presentation

Status: **Accepted**

Date: 2026-09-11

Decision owner: project owner, who reported that trees, bushes and rock all looked like the same object stamped repeatedly.

## Context

The world read as copy-paste. The owner's description — forests like a schoolchild's paint-bucket work, mountains like columnar rock with a jagged rim — turned out to be caused by two specific defects rather than by a shortage of art.

**The variant selector collapsed into a lattice.** The client picked a model with `(x * 37 + y) % 4`. Because 37 is congruent to 1 modulo 4, the multiplier did nothing and the expression reduced to `(x + y) % 4`: every cell on a diagonal received the same model, and the world was tiled in diagonal stripes four cells wide. Rock height had the same flaw. Its hash `(731x + 157y) % 7` reduced to `3(x + y) % 7`, because both 731 and 157 are congruent to 3 modulo 7.

**The variants were not distinguishable.** The four tree "variants" differed only by a 6% step in height and a small rotation of three thin branches. Crown shape, blob placement and colour were identical. At play distance that is one tree, not four — the owner reported only ever seeing a single variant, and that report was accurate.

**Instances were never posed.** `object_transform` applied translation only. Every tree faced the same direction, at the same scale, perfectly upright.

Rock relief compounded it: the ridge height was a fixed 2.65 and the cell centre varied between 2.80 and 3.10, a spread of about five percent, so a massif rendered as a flat plateau.

## Decision

### An instance is posed, and that comes first

A placed natural object receives a deterministic pose derived from its cell: a full-circle yaw, a bounded lean, and a scale that varies by up to 18% with a slight independent stretch in height. The mesh is unchanged and still shared, so a pose costs no extra mesh, no extra material and no extra draw batch.

This is deliberately the first lever. One mesh at many poses buys more apparent variety per unit of cost than any number of additional shapes, and it is why it lands before the shape work rather than after.

Bounds are explicit and tested: an instance stays upright to within the declared lean, keeps a square footprint, and never mirrors or inverts.

**Built things are never posed.** A wall mesh encodes its cardinal connectivity mask and a door's axis follows its neighbouring walls ([`ADR-0011`](0011-cardinal-connectivity-autotiles.md)), so turning one would destroy meaning rather than add variety. `ModelKind::accepts_pose_variety` names exactly which kinds may be posed, and a test asserts every structure kind is excluded. A carried stack is also unposed: its bearer's transform is owned by the animation.

### Variety comes from a mixed hash, not from arithmetic that looks mixed

Cell variety now runs the coordinates through the mixer the client already owns. A test pins the property that was broken rather than the implementation: no repetition along either axis at any period up to eight, no constant value along either diagonal, and an even distribution of shapes across a patch of world.

### Variants must differ in silhouette

Each natural kind now has variants that differ in proportion, part count and tone, not in scale alone. A tree is one of six individuals — tall and narrow, low and broad, two-tiered, leaning, young, or old and spreading — each with its own trunk ratio, crown blob count, spread and leaf tone. Bushes differ by lobe count, outcrops by boulder count and massing, veins by where the metal surfaces.

A test enforces this rather than trusting the author's eye: for every posed kind, each pair of variants must differ either in triangle count or in outline proportion by a visible margin. The 6%-height-step version of the tree would fail it.

### The variant space stays bounded, per kind

[`ADR-0005`](0005-procedural-visual-assets-as-code.md) requires a bounded variant space so a large world cannot demand one mesh per cell. That requirement is unchanged, but the bound is now per kind rather than a single global four: numerous, closely spaced things carry more shapes than rare ones. Trees have six, bushes and rock five, loose stacks three, and structures keep sixteen connectivity masks. Tests assert every kind has a positive count, that no count exceeds sixteen, that the whole cache stays under 128 meshes, and that an out-of-range variety value wraps rather than allocating.

## Consequences

- A forest of six tree shapes at arbitrary rotations and scales reads as a forest. The repetition that remains is the honest kind — six species-less shapes — rather than a visible grid.
- Adding shapes is now a bounded, per-kind decision rather than a global constant, so a future crowded model can carry more without inflating rare ones.
- Tree species — birch, oak, spruce — are **not** addressed here. They are content, not presentation variants, and belong in the content registry so that a species can later differ in what it yields without restructuring. This decision is what makes that cheap: species will bring their own meshes, and each will carry its own variants and poses through the same mechanism.
- Mountain relief is **not** addressed here. Per-cell height jitter would not fix it; a massif needs a profile derived from a cell's distance to the edge of the rock mass. That work is deferred to arrive together with per-cell deposit quantities, so that mining a vein visibly lowers the mountain and the geometry is written once rather than twice.
- Nothing authoritative changed. The deterministic headless smokes produce byte-identical output, and rendered pose remains presentation-only under [`ADR-0019`](0019-primary-low-poly-client.md): it never feeds back into movement, occupancy or persistence.
