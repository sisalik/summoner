# Creature animation — design notes & learnings

Hard-won knowledge from the bipedal animation overhaul (2026-07). Read this
before touching `src/creature/locomotion.rs` or adding archetype drivers.

## Kinematic vs verlet

Two driver styles coexist, split per archetype:

- **Bipedal drivers are fully kinematic**: every point's `pos` is written
  directly each tick and `prev_pos = pos`. No `verlet_integrate`, no
  `apply_constraints`, no `restore_vertical_center`. The pose is a pure
  function of `elapsed`, which makes `--render-creature --phase` stills exact
  and keeps planted feet rock-steady.
- **Other archetypes keep verlet + constraints** (quadruped, blob, winged,
  serpentine). Fine for wobbly bodies; wrong for gaits.

Lesson: verlet smoothing *fights* direct position writes — targets lag,
feet slide, everything reads as a ragdoll. Pick one regime per driver.
`restore_vertical_center` cancels mean-Y motion, so it silently deletes any
intentional body bob; never combine it with a gait.

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

## Arms: same principle

Fixed hand baselines (e.g. "carry hands 2px above rest") permanently bend
the elbows — same robotic read as the constant crouch. Instead the hand
traces an **arc from the shoulder** at radius ≈ arm length; the elbow gets
`give` (radius pull-in) only on the forward swing. Arms counter-swing the
same-side leg (armL in phase with legR).

## Two-bone IK bend direction

`solve_two_bone_ik_dir(..., bend_dir)`: with y-down and target below anchor,
`bend_dir = +1` puts the joint toward +x, `−1` toward −x. Front view: left
limbs −1, right limbs +1 (knees/elbows splay outward). Profile walk (facing
+x): knees `+1` (forward), elbows `−1` (backward). `set_state` restores rest
`bend_dir`s, limb widths, and point widths — any driver that overrides them
per tick relies on that.

## Per-creature personality

`GaitStyle` / `IdleStyle`: sampled once in `LocomotionState::new` by hashing
the skeleton's rest pose (FNV-1a over position/width bits) into an Xorshift
seed, then drawing each parameter from a tuned range. Properties:

- Deterministic per creature across restarts, **no seed plumbing** through
  the app needed.
- The two styles hash with flipped bits so gait and idle temperament don't
  correlate.
- Rare quirks are probability-gated draws: limp (~30%), arm flail (~20%),
  idle toe-tap (~40%), sleep twitch (~25%). Quirks are what make creatures
  memorable; continuous ranges alone blur together.
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
