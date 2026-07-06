# Creature animation — design notes & learnings

Hard-won knowledge from the bipedal animation overhaul and the all-archetype
rework (2026-07). Read this before touching `src/creature/locomotion/` or
adding archetype drivers.

## Module layout

`src/creature/locomotion/` is a directory module: `mod.rs` holds
`LocomotionState`, the state dispatch, and shared helpers (`style_rng`,
`pick`, `ease`, `lerp`, `put_point`); each archetype has its own file
(`bipedal.rs`, `quadruped.rs`, `winged.rs`, `blob.rs`, `serpentine.rs`)
containing its personality struct and its four state drivers as
`impl LocomotionState` blocks.

## Kinematic only — verlet is for ragdolls

**Every archetype driver is fully kinematic**: each point's `pos` is written
directly each tick and `prev_pos = pos` (use `put_point`). No
`verlet_integrate`, no `apply_constraints`, no vertical re-centering. The
pose is a pure function of `elapsed`, which makes `--render-creature
--phase` stills exact and keeps planted feet rock-steady. The only remaining
verlet user is the Disconnected settle (a one-shot collapse).

Lesson: verlet smoothing *fights* direct position writes — targets lag,
feet slide, everything reads as a ragdoll. The old `restore_vertical_center`
cancelled mean-Y motion, silently deleting any intentional body bob; it's
gone — don't reintroduce it.

## Walking = inverted pendulum

A constant crouch (fixed hip drop) keeps both knees bent through the whole
cycle and reads as a robot-dog skulk. Real gait: the body **vaults over the
planted stance leg** — hip height is `sqrt(L_eff² − dx²)` where `dx` is the
stance foot's horizontal offset from the hips. Consequences that fall out
for free:

- Knee straightens at the passing pose, flexes at contact and in swing.
- Pelvis bob emerges from geometry at 2× stride frequency — no synthetic
  sine needed. It is physically tiny (~0.3px at this scale) so it's scaled
  up by a personality factor to read at 18×24.
- Long striders bob more than mincing steppers; limpers bob asymmetrically.
- IK can never over-reach in stance: the target distance is `L_eff ≤ L` by
  construction. No straight-leg pops.

During double support (duty > 0.5 guarantees overlap), the **higher** of the
two pendulums carries the body. Keep `duty ≥ 0.54` so at least one leg is
always in stance.

Treadmill form (creature walks in place): stance foot slides back
*linearly*, swing foot returns forward on a sine arc with lift. Reference
poses: Richard Williams' contact / down / passing / up.

The quadruped reuses this wholesale, but with **one pendulum per girdle**:
front legs carry `front_body`, rear legs carry `rear_body`, so the spine
rocks naturally as pairs land. Leg phase offsets pick the gait — 4-beat walk
`[0, .5, .75, .25]`, trot `[0, .5, .5, 0]`, pace `[0, .5, 0, .5]` (comedy
waddle) — and each girdle's pair stays 0.5 apart so it always has a stance
leg.

## Arms: same principle

Fixed hand baselines (e.g. "carry hands 2px above rest") permanently bend
the elbows — same robotic read as the constant crouch. Instead the hand
traces an **arc from the shoulder** at radius ≈ arm length; the elbow gets
`give` (radius pull-in) only on the forward swing. Arms counter-swing the
same-side leg (armL in phase with legR).

## Archetype pose tricks that worked

- **Blob scaling is bottom-anchored**: hops, breathing, and the sleep-melt
  all scale point offsets about the lowest rest point
  (`y = bottom − (bottom − rest_y) * sy`), so the blob stays glued to its
  ground line. Widths swell as `sy` shrinks — the fake volume conservation
  is what sells squash/stretch. The blob is the one archetype with real
  headroom, so its waiting hops genuinely leave the ground (clamp hop height
  to `top_rest_y − 1.5`).
- **Serpentine periscope**: raise the head as a rigid-length angle chain
  from a mid-body point (`θ = π + curl`, stepping exact segment lengths),
  never by offsetting Y per point — offsets stretch the neck visibly.
  Scanning sway is an angle added down the chain, snake-charmer style.
- **Serpentine sleep coil**: build spiral targets from the tail inward,
  advancing `θ += seg_len / r` (arc-length-preserving) while shrinking `r`,
  then lerp rest→target with the ease. Slither amplitude should *grow*
  toward the tail (~0.3→1.0); the old even taper read as a wobbling stick.
- **Winged hover**: body bob is downward-only (no headroom) and
  counter-phased to the flap; the head follows only ~25% of it (birds
  stabilize their heads — this one cue makes the hover read). Feet tuck to
  ~55% of leg reach so the knees fold visibly. Wing flap: amplitude and
  phase delay grow toward the tip.
- **Quadruped sleep**: lerp everything to a lying pose (belly at
  `ground − 2.2`), but keep the head a distinct lump ~3.6px above the body
  line or the silhouette collapses into a single mound.
- **Serpentine tail-chase (waiting)**: wrap the body into a ring by placing
  each point at cumulative arc length around a circle (`ang = spin − arc/r`,
  `r = body_len / (2π·0.85)` leaving a chase gap), then spin the whole ring.
  The bright head wedge circling the gap is what sells "chasing its tail".

## Profile depth ordering

The rasterizer is a min-distance capsule union with no notion of front/back —
in a side view both arms and both legs collapse into the torso silhouette and
vanish. `Limb.depth` fixes this: the phase-1 SDF pass in `outline.rs` is
depth-aware. The frontmost capsule (max `depth`, ties broken by distance)
whose interior covers a pixel owns it, and a strictly-in-front capsule's
border band is stamped **even over** a deeper capsule's body — that interior
seam is what makes a limb read as being in front of the torso.

- `depth = 0` everywhere reduces exactly to the old union (no seams), so
  front-view archetypes (winged, blob, serpentine) and front-view bipedal
  states are unaffected.
- Bipedal *working* and all quadruped states set near limbs `+1`, far limbs
  `−1` (far ones also drawn ~0.8× width). Near limbs draw in front with a
  seam; far limbs are occluded behind the body.
- Depth is written per-tick by the driver and reset to the rest value (0) in
  `set_state`, so it never leaks into a front-view state.

## Wing membranes are filled panels

A wing is not just its bones. `collect_wing_membranes` fans triangles from the
body over the wing chain (the leading edge) **and closes on a flank vertex**
low on the body's side, so the fill spans the whole area beneath the struts.
Without the flank the membrane is a thin sliver along the bones and reads as a
stick. The membrane is rebuilt from live point positions each raster, so it
flaps with the bones for free.

## Two-bone IK bend direction

`solve_two_bone_ik_dir(..., bend_dir)`: with y-down and target below anchor,
`bend_dir = +1` puts the joint toward +x, `−1` toward −x. Front view: left
limbs −1, right limbs +1 (knees/elbows splay outward). Profile walk (facing
+x): knees `+1` (forward), elbows `−1` (backward). `set_state` restores rest
`bend_dir`s, limb widths, and point widths — any driver that overrides them
per tick relies on that.

## Per-creature personality

One style struct per archetype (`GaitStyle`+`IdleStyle` for bipedal,
`QuadStyle`, `WingStyle`, `BlobStyle`, `SnakeStyle`): sampled once in
`LocomotionState::new` by hashing the skeleton's rest pose (`style_rng`:
FNV-1a over position/width bits) into an Xorshift seed, then drawing each
parameter from a tuned range. Properties:

- Deterministic per creature across restarts, **no seed plumbing** through
  the app needed.
- Each struct salts the hash differently so temperaments don't correlate
  across states or archetypes.
- Rare quirks are probability-gated draws: limp (~25-30%), arm flail (~20%),
  idle toe-tap (~40%), sleep twitch (~25%), quadruped play-bow (~35%) and
  dream-paddle (~30%), winged glide-pause (~30%) and feather-ruffle (~30%),
  blob jiggle (~30%), snake head-flick (~35%). Quirks are what make
  creatures memorable; continuous ranges alone blur together. Ramp every
  gate window with `ease` so quirks fade in instead of popping.
- `Xorshift::new(0)` is a fixed point — always seed with `h | 1`.

Rate/phase irregularity must be a **bounded phase jitter**
(`sin` term added to phase), not a multiplier on `elapsed` — a rate multiplier
compounds unboundedly with time and the gait eventually thrashes.

## Canvas constraints (18×24 logical px, 18×12 terminal cells)

- The head rests at y=2 with radius ~2: it already touches the canvas top.
  **The body can never move up** — bounces must be downward knee-dips, hops
  must be feet-tucks. Rising crops the head flat.
- Feet cap at y=22.5 so the 1px SDF border stays on-canvas.
- Motion < ~0.3px only shimmers SDF edges; ≥ ~0.8px reads as movement.
- Profile poses need the torso narrowed (~0.65×) or limbs vanish inside the
  front-view silhouette.

## Non-looping feel

Combine incommensurate periods (0.23 Hz tilt + 1.2 Hz bounce + 0.17 Hz hop
gate...) so composite motion never visibly repeats. Rare events = slow sine
gate with a high threshold (`sin(...) > 0.92`), ramped inside the gate window
to avoid pops.

## Verification workflow

```bash
# one-shot still: --phase P runs P*60 ticks of 33ms ≈ 1.98*P seconds sim time
cargo run --features dev-creature -- --render-creature \
  --archetype bipedal --state working --seed 42 --phase 0.45 --scale 16 -o /tmp/c.png

# interactive: zoom (Enter), pause (space), frame-step (. ,), wireframe (c)
cargo run --features dev-creature -- --test-creatures
```

- Check pose stills at contact and passing, and always across several seeds
  (42, 7, 99, 555) — `vary()` extremes expose IK reach and crop bugs that
  the default seed hides.
- Frame-step in wireframe mode to confirm bend directions and that planted
  feet don't shimmer.
- `tests/creature_test.rs` guards: sleeping stays near-static (<2px per
  330ms), working must move the head, state changes reset the pose.
