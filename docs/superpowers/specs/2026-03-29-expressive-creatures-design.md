# Expressive Creature System

Replace the mask-based sprite generation and rigid whole-sprite animations with a point-chain physics system that produces organic, skeleton-aware procedural animation. Creatures become larger (~18x12 terminal cells), anatomically aware, and animate fluidly based on session state.

## Decisions

- **Size**: ~18x12 terminal cells (up from 12x7), half-block rendering gives ~18x24 effective sub-pixels
- **Skeleton model**: Point-chain physics (Verlet integration + distance constraints), not bone rotation — avoids pixel-grid jitter
- **Animation driver**: Procedural/physics-driven rules, no keyframes
- **Shape generation**: Chain → outline → fill (replaces mask/template system)
- **Variation**: Wide variation within archetypes via PRNG-seeded body parameters
- **State animations**: Literal/representational (walking, sitting, sleeping, etc.)
- **ShellOnly**: Keeps existing terminal icon sprite — only Claude sessions get creatures

## Skeleton Definition

Each archetype defines a skeleton topology: a tree of named chain points with distance constraints and optional limbs.

### Archetype Topologies

**Bipedal**
- Spine: head → neck → upper_body → lower_body (4 points)
- Arms: 2-bone limbs from upper_body (upper_arm → lower_arm)
- Legs: 2-bone limbs from lower_body via hip points (upper_leg → lower_leg → foot)

**Quadruped**
- Spine: head → neck → front_body → rear_body (4 points, horizontal orientation)
- Legs: 4 × 2-bone limbs from front_body and rear_body
- Tail: chain of 3 points from rear_body

**Blob**
- Ring of 6-8 points forming a closed loop, no limbs
- Shape is the entire body — deformation comes from ring point movement

**Winged**
- Spine: head → neck → body (3 points)
- Wings: multi-segment chains (3 points each) from body, spreading outward
- Legs: 2 × 2-bone limbs from body

**Serpentine**
- Long chain of 6-8 points, no limbs
- Locomotion via sine-wave propagation along the chain

### Data Model

```rust
struct Skeleton {
    points: Vec<ChainPoint>,      // named points with position, prev_position, width
    constraints: Vec<Constraint>,  // distance constraints between point pairs
    limbs: Vec<Limb>,             // 2-bone IK limbs attached to chain points
}

struct ChainPoint {
    name: &'static str,
    pos: Vec2,
    prev_pos: Vec2,
    width: f32,         // body outline width at this point
    pinned: bool,       // true for feet during stance
}

struct Constraint {
    a: usize,           // index into points
    b: usize,
    rest_length: f32,
}

struct Limb {
    anchor: usize,      // chain point this limb attaches to
    upper_len: f32,
    lower_len: f32,
    end_effector: Vec2,  // foot/hand target position
    side: Side,          // Left or Right (for mirroring)
}
```

## PRNG-Driven Body Variation

The creature seed (u64, same as current system) parameterizes the skeleton within archetype bounds:

| Parameter | Range (relative to archetype default) | Effect |
|-----------|--------------------------------------|--------|
| Segment lengths | ±30% | Taller/shorter torso, longer/shorter neck |
| Body widths | ±40% | Stocky vs lanky build |
| Limb proportions | upper:lower ratio 0.6–1.4 | Gangly vs compact limbs |
| Head size | ±30% | Small-headed to big-headed |
| Limb length | ±25% | Short stubby legs vs long striders |
| Asymmetry | 0–10% left/right difference | Subtle imperfection for organic feel |

Two bipedals with different seeds will have the same topology (4-point spine, 2 arms, 2 legs) but noticeably different proportions — one might be stocky with short legs and a wide torso, another lanky with a small head and long arms.

## Physics Simulation

### Per-Tick Update (runs at render frame rate)

1. **Drive**: Locomotion logic sets target velocities/positions for the lead point based on session state
2. **Verlet integrate**: For each non-pinned point: `new_pos = pos + (pos - prev_pos) * damping + forces * dt²`
3. **Constrain**: Enforce distance constraints between connected points (2-3 solver iterations)
4. **IK solve**: 2-bone IK (law of cosines) plants each limb's end effector at its target position
5. **Snap**: Round all positions to integer grid with hysteresis (only snap when floating-point position is >0.7px from current grid cell)

### 2-Bone IK

For a limb with upper bone length A, lower bone length B, and target at distance C from anchor:
- Knee/elbow angle: `acos((A² + B² - C²) / (2·A·B))`
- Solved in one step, no iteration needed
- At 18x24 sub-pixels, legs are 2-4 sub-pixels — this is sufficient

### Verlet Integration

```
velocity = (pos - prev_pos) * damping
prev_pos = pos
pos = pos + velocity + acceleration * dt²
```

Damping per state: Working=0.98, Waiting=0.95, Idle=0.90, Sleeping=0.85, Disconnected=0.0 (frozen).

### Distance Constraints

```
delta = b.pos - a.pos
current_len = delta.length()
correction = (current_len - rest_length) / current_len * 0.5
a.pos += delta * correction
b.pos -= delta * correction
```

2-3 iterations per tick is sufficient for chains of 4-8 points.

## Rendering Pipeline

Each frame, convert the physics state into a `Sprite` (the existing output type):

1. **Allocate sprite grid**: 18 wide × 24 tall sub-pixels, all Empty. (The half-block renderer packs 2 sub-pixel rows per terminal row, so 24 sub-pixel rows = 12 terminal rows, matching `CREATURE_HEIGHT`.)
2. **Draw body outline**: For spine-based archetypes (bipedal, quadruped, winged, serpentine): at each chain point, compute left/right boundary positions at ±(width/2) perpendicular to the chain direction. Connect consecutive boundary points with Bresenham lines. For blob: connect the ring points directly as a polygon outline.
3. **Draw limb outlines**: For each bone in each limb, draw a 1-2 sub-pixel wide line along the bone
4. **Draw head**: Small filled circle at head point (radius from PRNG parameters)
5. **Fill interior**: Scanline fill — for each row, find leftmost and rightmost outline pixels, fill between as Body
6. **Edge detect**: Body cells adjacent to Empty become Border (reuse existing `neighbors` logic)
7. **Output**: `Sprite { width: 18, height: 24, cells }` — fed into existing half-block renderer

The renderer (`render.rs`) and palette system are unchanged. The output contract is the same `Sprite` with `CellKind::{Body, Border, Empty}`.

## State Animations

| State | Lead Point Behavior | Limbs | Body |
|-------|-------------------|-------|------|
| **Working** | Oscillates horizontally (walk cycle) | Balance-based stepping: when center of mass drifts past support polygon, trailing foot steps forward along a Bezier arc with sigmoid easing. Arms swing in opposition to legs. | Slight forward lean, spine follows head with natural lag |
| **Waiting** | Small lateral drift | Feet planted, arms relaxed at sides | Head drifts side to side (looking around), subtle weight shift between feet |
| **Idle** | Stationary, lowered | Tucked close to body | Compressed spine (sitting posture), slow width oscillation (breathing), occasional small head movement |
| **Sleeping** | Stationary, lowest position | Fully tucked/curled | Spine folded or coiled (archetype-dependent), very slow rhythmic width expansion (breathing) |
| **Disconnected** | Stationary | Limp (constraints relaxed) | On entering this state: apply gravity once and run physics for ~10 ticks to settle into a collapsed posture, then freeze. No per-frame simulation. |
| **ShellOnly** | N/A | N/A | Existing `terminal_icon_sprite()` — no physics, no creature |

### Walk Cycle (Working state, detail)

Based on Rain World's balance-based approach:
1. Track center of mass (average of spine points) relative to grounded feet
2. When center of mass X moves outside the support range (between planted feet), trigger a step
3. Trailing foot lifts and moves along a parabolic arc to a position ahead of the center of mass
4. Foot arc uses sigmoid easing: `1 / (1 + exp(-10 * (t - 0.5)))` for snappy lift-off and landing
5. Stride length scales with creature's leg length (from PRNG parameters)

Quadrupeds use the same logic with a diagonal gait (front-left + rear-right step together).
Serpentines propagate a sine wave along the chain — no foot IK.
Blobs contract/expand their ring asymmetrically to "roll" forward.
Winged creatures add wing flap (wing chain oscillation) layered on top of bipedal walk.

## File Structure Changes

### Replaced

| File | Current | New |
|------|---------|-----|
| `creature/generate.rs` | Xorshift PRNG, mask-based sprite generation, edge detection | Xorshift PRNG (kept), skeleton instantiation from archetype + seed, outline rasterization, fill, edge detection |
| `creature/templates.rs` | 5 mask arrays (i8 grids) | 5 archetype skeleton definitions (topology + default parameters + parameter ranges) |
| `creature/animate.rs` | Frame counter, rigid sprite transforms (shift, bounce, breathe, crop) | Verlet physics, distance constraints, 2-bone IK, state-driven locomotion logic |

### Kept (unchanged or minor adjustments)

| File | Notes |
|------|-------|
| `creature/render.rs` | Half-block renderer, palettes — unchanged. `sprite_cell_size` updated for new default size. `terminal_icon_sprite()` unchanged. |
| `session.rs` | SessionState enum, state detection — unchanged |
| `ui/dashboard.rs` | `CREATURE_WIDTH` → 18, `CREATURE_HEIGHT` → 12 (was 12, 8). Card layout math adjusts. |
| `ui/dashboard_nav.rs` | Unchanged |
| `ui/status_bar.rs` | Unchanged |
| `config.rs` | SessionStore already has `creature_seed` and `creature_template` — both still used |

### New modules (within existing files or small new files)

- `creature/physics.rs` — Verlet integration, distance constraint solver, 2-bone IK. ~100-150 lines.
- `creature/outline.rs` — Chain-to-outline conversion, scanline fill, Bresenham line drawing. ~100-150 lines.

## Migration

- Existing sessions retain their `creature_seed` and `creature_template` — these map directly to the new system (seed parameterizes the skeleton, template selects the archetype)
- No config file changes needed
- No new dependencies — all math is basic Vec2 operations, trig for IK
