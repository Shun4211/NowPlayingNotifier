//! 同じ曲の通知が何度来てもポップアップを1回だけ出すための判定。

#[derive(Debug, PartialEq, Eq)]
pub enum TrackChange {
    /// 曲名・アーティストが空（表示するものがない）。
    Empty,
    /// 直前と違う曲になった。
    NewTrack,
    /// 直前と同じ曲（サムネイル更新などによる再通知）。
    SameTrack,
}

#[derive(Default)]
pub struct TrackChangeDetector {
    last: Option<(String, String)>,
}

impl TrackChangeDetector {
    pub fn evaluate(&mut self, title: &str, artist: &str) -> TrackChange {
        if title.trim().is_empty() && artist.trim().is_empty() {
            return TrackChange::Empty;
        }
        if self.last.as_ref().is_some_and(|(t, a)| t == title && a == artist) {
            return TrackChange::SameTrack;
        }
        self.last = Some((title.to_string(), artist.to_string()));
        TrackChange::NewTrack
    }

    /// セッションがなくなったときなどに、次の曲を必ず新しい曲として扱うようにする。
    pub fn reset(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_track_is_new() {
        assert_eq!(TrackChangeDetector::default().evaluate("Song", "Artist"), TrackChange::NewTrack);
    }

    #[test]
    fn repeated_notification_for_same_track_is_same() {
        let mut d = TrackChangeDetector::default();
        d.evaluate("Song", "Artist");
        assert_eq!(d.evaluate("Song", "Artist"), TrackChange::SameTrack);
        assert_eq!(d.evaluate("Song", "Artist"), TrackChange::SameTrack);
    }

    #[test]
    fn different_title_or_artist_is_new() {
        let mut d = TrackChangeDetector::default();
        d.evaluate("Song", "Artist");
        assert_eq!(d.evaluate("Song", "Other Artist"), TrackChange::NewTrack);
        assert_eq!(d.evaluate("Other Song", "Other Artist"), TrackChange::NewTrack);
    }

    #[test]
    fn blank_info_is_empty_and_does_not_replace_last_track() {
        for (title, artist) in [("", ""), ("  ", "")] {
            let mut d = TrackChangeDetector::default();
            d.evaluate("Song", "Artist");
            assert_eq!(d.evaluate(title, artist), TrackChange::Empty);
            assert_eq!(d.evaluate("Song", "Artist"), TrackChange::SameTrack);
        }
    }

    #[test]
    fn after_reset_same_track_is_new_again() {
        let mut d = TrackChangeDetector::default();
        d.evaluate("Song", "Artist");
        d.reset();
        assert_eq!(d.evaluate("Song", "Artist"), TrackChange::NewTrack);
    }
}
