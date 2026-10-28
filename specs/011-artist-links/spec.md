# Artist links in track metadata

## Goal
Make artist credits beneath playlist tracks and the currently playing track discoverable and
navigable to the existing artist profile.

## Acceptance criteria
- Artist credits in playlist track rows underline on hover and navigate to the matching artist profile when clicked.
- Artist credits in the player bar behave the same way.
- Clicking an artist credit does not trigger track playback or other parent-row actions.
- Credits without a verified artist URI remain plain text.
- `cargo fmt --check`, `cargo check`, and `cargo test` pass.
