use std::ops::Range;

use crate::lib::spotify::library::{LibraryEntry, LibraryKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum LibraryView {
    Compact,
    #[default]
    List,
    CompactGrid,
    Grid,
}

impl LibraryView {
    pub const ALL: [Self; 4] = [Self::Compact, Self::List, Self::CompactGrid, Self::Grid];
    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact list",
            Self::List => "List",
            Self::CompactGrid => "Compact grid",
            Self::Grid => "Grid",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::Compact => "icons/compact-list.svg",
            Self::List => "icons/list.svg",
            Self::CompactGrid => "icons/compact-grid.svg",
            Self::Grid => "icons/grid.svg",
        }
    }
    pub fn is_grid(self) -> bool {
        matches!(self, Self::CompactGrid | Self::Grid)
    }
    pub fn columns(self, width: f32) -> usize {
        if self.is_grid() {
            ((width - 16.0) / 144.0).floor().max(1.0) as usize
        } else {
            1
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Filter {
    Playlists,
    Podcasts,
    Albums,
    Artists,
}

impl Filter {
    pub const ALL: [Self; 4] = [Self::Playlists, Self::Podcasts, Self::Albums, Self::Artists];
    pub fn label(self) -> &'static str {
        match self {
            Self::Playlists => "Playlists",
            Self::Podcasts => "Podcasts",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
        }
    }
    pub fn kind(self) -> LibraryKind {
        match self {
            Self::Playlists => LibraryKind::Playlist,
            Self::Podcasts => LibraryKind::Show,
            Self::Albums => LibraryKind::Album,
            Self::Artists => LibraryKind::Artist,
        }
    }
    pub fn matches(self, kind: LibraryKind) -> bool {
        if self == Self::Playlists {
            matches!(
                kind,
                LibraryKind::Playlist | LibraryKind::LikedSongs | LibraryKind::Folder
            )
        } else {
            kind == self.kind()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Ownership {
    #[default]
    All,
    ByYou,
    BySpotify,
}

impl Ownership {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::ByYou => "By you",
            Self::BySpotify => "By Spotify",
        }
    }
    pub fn matches(self, entry: &LibraryEntry, username: &str) -> bool {
        match self {
            Self::All => true,
            Self::ByYou => {
                entry.kind == LibraryKind::LikedSongs
                    || (!username.is_empty() && entry.owner_username == username)
            }
            Self::BySpotify => entry.owner_username.eq_ignore_ascii_case("spotify"),
        }
    }
}

/// Return original UTF-8 byte ranges, including Unicode lowercase expansions (İ -> i + dot).
pub(super) fn match_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut folded = String::new();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    for (start, character) in text.char_indices() {
        for lower in character.to_lowercase() {
            folded.push(lower);
            starts.extend(std::iter::repeat_n(start, lower.len_utf8()));
            ends.extend(std::iter::repeat_n(
                start + character.len_utf8(),
                lower.len_utf8(),
            ));
        }
    }
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (start, matched) in folded.match_indices(&query) {
        let range = starts[start]..ends[start + matched.len() - 1];
        if let Some(previous) = ranges
            .last_mut()
            .filter(|previous| previous.end >= range.start)
        {
            previous.end = previous.end.max(range.end);
        } else {
            ranges.push(range);
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_modes_have_distinct_semantics_and_safe_columns() {
        assert_eq!(LibraryView::ALL.len(), 4);
        assert!(!LibraryView::Compact.is_grid());
        assert!(LibraryView::CompactGrid.is_grid());
        assert_eq!(LibraryView::Grid.columns(308.0), 2);
        assert_eq!(LibraryView::Grid.columns(1136.0), 7);
        assert_eq!(LibraryView::Grid.columns(20.0), 1);
        assert_eq!(LibraryView::List.columns(1400.0), 1);
    }

    #[test]
    fn highlights_are_case_insensitive_and_unicode_safe() {
        assert_eq!(match_ranges("Indie indigo", "ind"), vec![0..3, 6..9]);
        assert_eq!(match_ranges("Été", "é"), vec![0..2, 3..5]);
        assert_eq!(match_ranges("İstanbul", "i"), vec![0..2]);
        assert!(match_ranges("Anything", "  ").is_empty());
        assert!(match_ranges("Anything", "missing").is_empty());
    }

    #[test]
    fn ownership_uses_account_identity_not_decorated_display_name() {
        let mut entry = LibraryEntry::liked_songs("fixture-account");
        entry.kind = LibraryKind::Playlist;
        entry.owner = "Same display name".into();
        assert!(Ownership::ByYou.matches(&entry, "fixture-account"));
        assert!(!Ownership::ByYou.matches(&entry, "other-account"));
        entry.owner_username = "spotify".into();
        assert!(Ownership::BySpotify.matches(&entry, "fixture-account"));
    }
}
