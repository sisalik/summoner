# Creature Outlines & Debug Tooling — Design Spec

**Date:** 2026-03-29
**Status:** Draft

## Problem

The current outline algorithm (`outline.rs`) draws Bresenham lines along spine boundary edges, then scanline-fills everything between the leftmost and rightmost border pixels on each row. This causes:

- **Legs merge together** — scanline fill treats the gap between legs as interior
- **Arms disappear into the torso** — limb bones are 1px lines consumed by the body fill
- **Hip region is chaotic** — overlapping spine segments, hip points, and limb anchors all collide
- **No visual distinction between body parts** — flat single-color fill makes everything one blob

The skeleton structure itself is reasonable (verified in wireframe mode), but the outline/fill step loses all limb separation.

## Solution: SDF Capsule Outline Engine

Replace the Bresenham + scanline approach with **signed distance field (SDF) capsule evaluation**. Each bone segment becomes a capsule shape (line segment + radius at each endpoint). For every pixel in the grid, compute the minimum signed distance across all capsules:

```
dist = MIN over all capsules of sdf_tapered_capsule(pixel, start, end, r_start, r_end)

if dist <= 0.0:            → Body (record which capsule was closest)
if 0.0 < dist <= 1.0:      → Border
else:                       → Empty
```

### Why SDF Capsules

Evaluated against circle-stamping (Rain World style) and metaballs (Spore style):

- **Circle-stamping** creates visible scalloping at 18x24 resolution where circles don't perfectly overlap
- **Metaballs** offer smooth merging, but at 18x24 the 1-2 pixel transition zone rounds to the same result as a hard union — sophistication wasted
- **SDF capsules** give the cleanest edges, mathematically precise tapering, and built-in border width control via the distance threshold

Performance is negligible: ~5 FP ops per pixel per bone, ~20K total evaluations for a 10-bone creature on an 18x24 grid.

### Per-Archetype Capsule Strategy

**Bipedal:**
- Spine segments (head→neck→upper_body→lower_body) as tapered capsules using `ChainPoint.width` at each end
- 4 limb bones (2 per limb, from IK solve) as thin capsules with new `Limb.upper_width` / `Limb.lower_width` fields
- Head as a filled circle (capsule with zero length, just radius)
- Legs are clearly separated — each is its own capsule, no scanline fill merging them

**Quadruped:**
- Spine chain (head→neck→front_body→rear_body→tail×3) as tapered capsules
- 4 leg bones as thin capsules
- Head circle
- Tail tapers naturally via decreasing `ChainPoint.width`

**Blob:**
- Each ring segment (between adjacent ring points) as a capsule using the ring point widths
- The union of 8 capsules naturally forms a filled rounded shape
- Single shade — intentionally flat, it's a blob

**Winged:**
- Body spine (head→neck→body) as tapered capsules, same as bipedal
- 2 thin leg bones as capsules
- Wing membrane: for each wing, the membrane is a series of filled triangles — one triangle per wing segment, each formed by (body_anchor, wing_bone_N, wing_bone_N+1). Left wing: (body, lwing1, lwing2), (body, lwing2, lwing3). Right wing mirrored.
- Membrane pixels are evaluated separately from capsules: for each pixel, test if it falls inside any membrane triangle (barycentric coordinates). If yes and the pixel is currently Empty, mark as Body with a wing-membrane capsule ID. This runs after capsule evaluation so body capsules take priority over membrane.

**Serpentine:**
- Chain of 8 tapered capsules, widths tapering from head to tail
- Same algorithm as spine capsules, just more segments
- Subtle head→tail shade gradient for directionality

### SDF Functions

**Tapered capsule SDF** (uneven radii at each end):
```
sdf_tapered_capsule(p, a, b, r_a, r_b):
    ab = b - a
    ap = p - a
    t = clamp(dot(ap, ab) / dot(ab, ab), 0.0, 1.0)
    closest = a + ab * t
    radius = lerp(r_a, r_b, t)
    return distance(p, closest) - radius
```

**Union:** `min(sdf1, sdf2, ...)`

**Border:** `0.0 < dist <= border_width` where `border_width = 1.0` sub-pixel.

### Skeleton Changes

Add limb width fields to the `Limb` struct:

```rust
pub struct Limb {
    pub anchor: usize,
    pub upper_len: f32,
    pub lower_len: f32,
    pub upper_width: f32,  // NEW — radius of upper bone capsule
    pub lower_width: f32,  // NEW — radius of lower bone capsule
    pub end_effector: Vec2,
    pub side: Side,
}
```

Default values: `upper_width` ~1.0-1.5, `lower_width` ~0.8-1.2, varied by PRNG per creature. This gives each creature slightly different limb thickness.

## Per-Capsule Shading

Each capsule is assigned a **shade level** within the session state's color palette. The SDF evaluation tracks which capsule produced the minimum distance, so every Body pixel knows its capsule ID.

### Shade Assignment

| Body part | Shade | Effect |
|-----------|-------|--------|
| Head | Lightest (+20% lightness) | Focal point |
| Torso/spine | Base color | Dominant mass |
| Upper limbs (arm/leg upper bone) | Slightly darker (-10%) | Distinct from torso |
| Lower limbs (arm/leg lower bone) | Darker (-20%) | Reads as separate segment |

### Per-Archetype Shading

- **Bipedal/Quadruped:** Full gradient — head lightest, spine base, limbs progressively darker
- **Winged:** Body+legs same as bipedal; wing membrane gets a distinct lighter tone
- **Blob:** Single flat shade — no differentiation
- **Serpentine:** Linear gradient from head (lightest) to tail (darkest)

### Data Flow

The rasterizer outputs both a `Sprite` (CellKind grid) and a parallel `Vec<u8>` of capsule IDs. The renderer uses capsule IDs to look up shade offsets from a per-archetype shade table.

This is the same pattern as the existing debug `rasterize_skeleton_debug()`, but now used in all rendering modes (not just debug).

### Color Modes

Three modes (the old flat `state` mode is removed):

| Mode | Description | Use |
|------|-------------|-----|
| `shaded` | State palette + per-capsule shade gradient | Default look for main app and test mode |
| `limb` | Rainbow colors per component ID | Debug: verify capsule assignments |
| `wireframe` | Skeleton lines + joints, no fill | Debug: verify bone positions |

## CLI Render Command

A non-interactive command that outputs a PNG image of a creature. For use in automated feedback loops (particularly for AI-assisted iteration on creature visuals).

### Usage

```
cargo run --features dev-creature -- --render-creature [OPTIONS]
```

### Options

| Flag | Values | Default | Description |
|------|--------|---------|-------------|
| `--archetype` | bipedal, quadruped, blob, winged, serpentine | bipedal | Creature archetype |
| `--state` | working, waiting, idle, sleeping, disconnected, rest | rest | Session state (rest = static skeleton) |
| `--seed` | u64 | 42 | PRNG seed for creature generation |
| `--phase` | 0.0-1.0 | 0.0 | Animation phase — simulates N ticks to reach this fraction of a cycle |
| `--color` | shaded, limb, wireframe | shaded | Color/render mode |
| `--scale` | integer | 16 | Pixel scale factor (16 → 288x384px output) |
| `-o` | file path | creature.png | Output file path |

### Implementation

1. Parse args, instantiate `Skeleton` from archetype+seed
2. If phase > 0: create `LocomotionState`, tick it `phase * cycle_ticks` times at 33ms steps
3. Rasterize skeleton to sprite + capsule IDs at given scale
4. Map sprite cells + capsule IDs to RGB using selected color mode
5. Scale each sub-pixel to a `scale × scale` block of pixels
6. Write PNG via `png` crate
7. Exit (no TUI, no terminal setup)

### Output

PNG format. At default 16x scale: 288×384 pixels. Large enough for vision model evaluation, small enough for quick rendering.

## Feature Gating

All creature test and render tooling is behind a cargo feature so it's excluded from release builds:

```toml
[features]
default = []
dev-creature = ["dep:png"]

[dependencies]
png = { version = "0.17", optional = true }
```

### What's behind `dev-creature`:

- `--render-creature` CLI command and all its argument parsing
- `--test-creatures` interactive TUI mode
- `test_creatures.rs` module
- PNG output code
- The `png` crate dependency

### What's NOT behind `dev-creature`:

- `outline.rs` SDF rasterizer (used by main app)
- `skeleton.rs`, `physics.rs`, `locomotion.rs` (core creature system)
- `render.rs` half-block renderer (used by main app)
- Per-capsule shading (used by main app dashboard)

## Interactive TUI Enhancements

Enhancements to the existing `--test-creatures` zoom mode (behind `dev-creature` feature):

### New Controls (Zoom Mode)

| Key | Action |
|-----|--------|
| `Space` | Pause/resume animation |
| `.` | Step one frame forward (when paused) |
| `,` | Step one frame backward (when paused) |
| `c` | Cycle color mode: shaded → limb → wireframe |

Frame-stepping backward re-simulates from t=0 to t_current-1 (recreate skeleton, tick N-1 times). At 18x24 with simple physics this is negligible.

### Existing Controls (Unchanged)

| Key | Action |
|-----|--------|
| `Left/Right` | Cycle session state |
| `Up/Down` | Cycle archetype |
| `r` | Randomize seed |
| `z` / `Esc` | Back to grid |
| `q` | Quit |

### Grid Mode

Unchanged. Uses shaded color mode by default (was `state`).

## Files Changed

| File | Change |
|------|--------|
| `src/creature/outline.rs` | Replace Bresenham+scanline with SDF capsule evaluation. All rasterize functions rewritten. |
| `src/creature/skeleton.rs` | Add `upper_width`/`lower_width` to `Limb`. Populate from PRNG in each `build_*` function. |
| `src/creature/render.rs` | Add shade offset lookup. Accept capsule IDs in rendering. Remove flat `state` palette path. |
| `src/creature/mod.rs` | Update exports if needed. |
| `src/test_creatures.rs` | Add pause/frame-step. Remove `State` color mode. Feature-gate the module. |
| `src/main.rs` | Add `--render-creature` arg parsing (feature-gated). PNG output function. |
| `Cargo.toml` | Add `dev-creature` feature, optional `png` dependency. |
| `src/ui/dashboard.rs` | Pass capsule IDs to renderer (use shaded mode). |

## Out of Scope

- Animation changes (locomotion, physics, walk cycles) — this spec only fixes how skeletons become pixels
- New archetypes
- Config-file-driven creature customization
