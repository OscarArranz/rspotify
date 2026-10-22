# Restore the last-known track — design

librespot's Connect task receives the server-side `hm://connect-state/v1/cluster` stream and the
cluster returned while registering the device. A narrowly patched local copy of librespot-connect
exposes a read-only snapshot of that received player state through `Spirc`.

The native client polls that snapshot off the GPUI foreground thread. When a server track URI changes,
it resolves display metadata with the existing native extended-metadata client and updates the shared
playback snapshot. The client does not activate, transfer, or resume playback. Its own player events
continue to govern state when rspotify is the active device. No playback data is written locally.

An explicit play action loads the displayed track (and its context when available) into the local
Connect device at the server position, after activating rspotify. Space invokes the same toggle only
when no text input owns focus; held/repeated key-down events and modified Space combinations are
ignored.
