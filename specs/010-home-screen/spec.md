# Home screen

## Outcome

Replace the welcome placeholder with a personalized Spotify-style Home, populated by Spotify's native desktop services. Playback requires Spotify Premium.

## Acceptance criteria

- Home displays available personalized shortcuts, recommendation shelves and featured panels, with responsive layouts and artwork.
- Category filters, shelf navigation, item navigation and playback work when supported by the feed and existing client.
- Video-backed recommendation panels play supported previews, with static artwork when video is unavailable; no DRM bypass.
- Loading, empty and failure states are clear, and unsupported feed content or actions are not presented as working.
- Network work never blocks the UI; no public Web API or personal data in fixtures or logs.
- Home works after session restoration and authentication, and navigation back to it is sensible.

## Validation

Use fixture-based tests and the standard Rust validation commands. Real-account and visual checks require explicit user authorization.
