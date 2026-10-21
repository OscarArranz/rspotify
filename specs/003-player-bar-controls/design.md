# Interactive player bar — design

## Module boundaries

- `src/view/main_layout.rs` owns slider interaction state, pointer mapping, tooltip, and rendering.
- `src/lib/spotify/client.rs` already exposes `seek(position_ms)` and
  `set_volume_percent(percent)`; these remain the command boundary.

## Behavior

The progress slider derives its fill from the playback snapshot. Pointer coordinates are normalized
against the slider's actual layout bounds and clamped to `[0, 1]`. Hover changes only the tooltip;
the playhead stays at the current playback position. During a seek drag, the playhead previews the
pointer position, and the seek command is sent once on mouse release. The hover tooltip formats the
normalized pointer position multiplied by track duration. Seeking is disabled when duration is zero.

The volume slider uses the same pointer-to-bounds mapping and converts the normalized value to an
integer percentage. While dragging, it updates the local SoftMixer directly for immediate audible
feedback and updates the UI preview; it does not send Connect commands per pointer event. On release,
it sends one `set_volume_percent` command to commit the level. Snapshot polling remains the source
of truth for the committed volume outside a drag. GPUI pointer tracking must use layout bounds rather
than assumed window dimensions so resizing does not change slider semantics.

After seek or volume commit, the UI keeps a local optimistic target until the playback snapshot
reports a matching value or three seconds elapse. Optimistic seek display advances with elapsed
time while playback is active and is scoped to the track URI; on mismatch timeout/track change it
falls back to the server snapshot.

Keep GPUI interaction state in the player-bar entity. Do not block the foreground thread or add
dependencies. No account/network smoke test is run without explicit approval.

Both controls are rendered by one `player_slider` helper with value-specific fill color and optional
tooltip. On hover, the actual fill remains fixed and a lighter segment previews the pointer target.
During drag, the thumb previews the clamped pointer position; seek is committed on mouse-up, while
volume is applied continuously. The player-bar mouse handlers continue tracking drag movement while
the pointer is outside the narrow slider but still within the bar.
