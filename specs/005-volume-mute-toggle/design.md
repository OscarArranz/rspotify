# Volume mute toggle — design

## Boundaries

- `PlayerBar` owns remembered volume and the button's hover state in `src/view/main_layout.rs`.
- `NativeSpotify::set_volume_percent` remains the command boundary.
- SVG artwork lives under `assets/icons/` and is selected based on displayed volume.

## Behavior

Treat a volume of zero as muted. On mute, retain the last nonzero volume and commit zero. On unmute,
restore that retained level, defaulting to 50%. Slider drags above zero update the remembered level;
the existing optimistic slider behavior remains unchanged. Icon hover changes its color from muted
gray to white and the button exposes a Mute/Unmute tooltip.
