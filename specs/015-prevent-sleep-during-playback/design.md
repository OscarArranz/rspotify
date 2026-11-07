# Sleep inhibition — design (spec 015)

## Ownership and behavior

The librespot player event loop is the authoritative source for local playback transitions. It updates `PlaybackSnapshot` on the same `Playing`, `Paused`, `Stopped`, and `Unavailable` events that govern UI state. A platform adapter observes the resulting boolean state and owns the inhibitor resource. The inhibitor blocks system sleep only; it must not request a display/idle inhibitor.

The adapter must be non-fatal and transition-driven: acquire on false → true, release on true → false, and release on drop. Failure to contact the OS service must not affect playback.

## Platform boundary

Implement system-specific sleep inhibition behind one small module. On Linux, use the session/system power-management inhibition mechanism with a reason identifying rspotify audio playback. Unsupported platforms are no-op until an equivalent implementation is available; keep this limitation explicit. Never use periodic synthetic input or alter display timeout settings.

## Validation

Exercise playback-event state transitions offline, including loading (not yet playing), paused, stopped, and unavailable. Verify acquired state follows only `snapshot.is_playing`. Run the application Rust validation commands. Live desktop sleep behavior remains a manual check.
