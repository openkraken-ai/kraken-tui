# TASK-K0 Animation Choreography Contract

## Scope

This contract defines the minimum choreography API and runtime semantics used by TASK-K11.
It is intentionally constrained to the MVP behavior required by Epic K:

- group lifecycle
- timeline offsets
- cancellation guarantees
- deterministic startup order

## FFI API

All functions use Kraken's standard status conventions:

- `0` success
- `-1` error (`tui_get_last_error`)
- handle-returning APIs return `0` on error

### Group lifecycle

- `tui_create_choreo_group() -> u32`
- `tui_destroy_choreo_group(u32 group) -> i32`

### Membership and timeline

- `tui_choreo_add(u32 group, u32 anim_handle, u32 start_at_ms) -> i32`

Rules:

- `anim_handle` must already exist.
- `group` must exist.
- adding the same animation twice to a group is rejected.
- adding to a running group is rejected.
- adding marks the animation as `pending` until scheduled by group timeline.

### Execution control

- `tui_choreo_start(u32 group) -> i32`
- `tui_choreo_cancel(u32 group) -> i32`

Rules:

- `start` begins timeline at `t=0`.
- members with `start_at_ms == 0` are activated immediately.
- scheduled activation is based on absolute offset from group start time.

## Runtime semantics

## Ordering

- group members are ordered by `start_at_ms` ascending.
- equal offsets are started in insertion order.

## Fan-out

- fan-out is represented by multiple members with the same `start_at_ms`.
- all such members become eligible in the same render advancement step.

## Fan-in

- fan-in is represented by multiple predecessor animations scheduled before a shared follower offset.
- MVP does not gate on predecessor completion; fan-in is timeline-based only.

## Cancellation

- cancelling a group prevents all not-yet-started members from starting.
- not-yet-started members are cancelled and removed from active animation registry.
- already-started members continue unless explicitly cancelled by caller.

## Lifecycle cleanup

- removing/cancelling an animation removes it from choreography membership.
- empty groups may be dropped internally.
- explicit `tui_destroy_choreo_group` is still the public lifecycle endpoint.

## Test obligations (K11)

- valid group lifecycle calls succeed.
- delayed follower does not start after group cancellation.
- zero-offset members start at group start.
- invalid group/animation handles return errors.
