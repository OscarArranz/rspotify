# Persist playback volume — design

The existing `SessionStore` owns the private `session.json` file, so add an optional volume field
there rather than creating another preference store. Its accessor returns 50% when the field is
missing (backwards compatibility) and clamps stored values to 100%. The existing atomic-ish
temporary-file write and Unix permissions remain in force.

During authentication, read the preference and pass it into `connect` to set
`ConnectConfig.initial_volume`. When `NativeSpotify::set_volume_percent` succeeds, load the store,
update the preference, and save it immediately. On startup, also apply the value directly to the
local soft mixer and seed `PlaybackSnapshot.volume_percent`, so both audible output and the player
bar reflect the restored preference before a player event arrives. This ensures changes survive an
unexpected exit without needing a shutdown hook. Session setup and UI boundaries remain unchanged.
