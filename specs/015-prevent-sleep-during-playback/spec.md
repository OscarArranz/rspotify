# Prevent sleep during playback (spec 015)

## User outcome

While rspotify is actively playing music, the operating system does not suspend the device due to inactivity. The display may turn off normally. Paused, stopped, unavailable, or disconnected playback must not hold a sleep inhibitor.

## Acceptance criteria

- Acquire a system sleep inhibitor only while local playback is active; do not inhibit display power-down.
- Release the inhibitor promptly when playback pauses, stops, becomes unavailable, or the app exits.
- Reconcile inhibitor state after playback state transitions and session reconnects; no stale inhibitor may survive a playback stop.
- If the platform inhibition service is unavailable, playback continues normally and rspotify does not crash.
- Document supported platform behavior and validate transition logic offline.
