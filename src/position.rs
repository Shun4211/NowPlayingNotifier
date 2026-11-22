//! ポップアップの表示位置の計算。

use crate::settings::PopupPosition;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// 作業領域の指定した隅に、画面端から `margin` だけ離してポップアップを置いたときの左上座標。
/// 単位はすべて同じ（物理ピクセル）であること。
pub fn calculate_position(position: PopupPosition, width: i32, height: i32, work: Rect, margin: i32) -> (i32, i32) {
    let left = work.left + margin;
    let right = work.right - width - margin;
    let top = work.top + margin;
    let bottom = work.bottom - height - margin;
    match position {
        PopupPosition::TopLeft => (left, top),
        PopupPosition::TopRight => (right, top),
        PopupPosition::BottomLeft => (left, bottom),
        PopupPosition::BottomRight => (right, bottom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1920x1040 の作業領域（下にタスクバー）
    const WORK: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1040 };
    const W: i32 = 320;
    const H: i32 = 120;
    const M: i32 = 20;

    #[test]
    fn corner_positions_use_same_margin_on_all_sides() {
        for (pos, x, y) in [
            (PopupPosition::TopLeft, M, M),
            (PopupPosition::TopRight, 1920 - W - M, M),
            (PopupPosition::BottomLeft, M, 1040 - H - M),
            (PopupPosition::BottomRight, 1920 - W - M, 1040 - H - M),
        ] {
            assert_eq!(calculate_position(pos, W, H, WORK, M), (x, y), "{pos:?}");
        }
    }

    #[test]
    fn zero_margin_touches_screen_edges() {
        assert_eq!(calculate_position(PopupPosition::TopLeft, W, H, WORK, 0), (0, 0));
        assert_eq!(calculate_position(PopupPosition::BottomRight, W, H, WORK, 0), (1920 - W, 1040 - H));
    }

    #[test]
    fn larger_margin_moves_popup_inward() {
        assert_eq!(calculate_position(PopupPosition::TopLeft, W, H, WORK, 50), (50, 50));
    }

    #[test]
    fn offsets_by_secondary_monitor_origin() {
        let secondary = Rect { left: 1920, top: 0, right: 1920 + 2560, bottom: 1400 };
        assert_eq!(calculate_position(PopupPosition::TopLeft, W, H, secondary, M), (1920 + M, M));
    }

    #[test]
    fn handles_monitor_with_negative_coordinates() {
        let left = Rect { left: -1280, top: 0, right: 0, bottom: 1024 };
        assert_eq!(calculate_position(PopupPosition::TopRight, W, H, left, M).0, -W - M);
    }

    #[test]
    fn larger_popup_moves_away_from_right_edge() {
        let (small, _) = calculate_position(PopupPosition::TopRight, 300, H, WORK, M);
        let (large, _) = calculate_position(PopupPosition::TopRight, 500, H, WORK, M);
        assert_eq!(small - large, 200);
    }
}
