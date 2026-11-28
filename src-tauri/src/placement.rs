//! Pure window-placement geometry: is the grip strip reachable on some monitor,
//! and where does the default position go. All values are physical pixels.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    fn right(&self) -> i64 {
        self.x as i64 + self.w as i64
    }

    fn bottom(&self) -> i64 {
        self.y as i64 + self.h as i64
    }

    /// Width and height of the overlap with `other` (zero when disjoint).
    fn overlap(&self, other: &Rect) -> (i64, i64) {
        let w = self.right().min(other.right()) - (self.x as i64).max(other.x as i64);
        let h = self.bottom().min(other.bottom()) - (self.y as i64).max(other.y as i64);
        (w.max(0), h.max(0))
    }
}

/// Logical size of the grip strip at the top of the widget, and how much of its
/// width must stay on screen for it to count as reachable.
pub const GRIP_HEIGHT: f64 = 30.0;
pub const GRIP_MIN_VISIBLE_WIDTH: f64 = 60.0;
/// Gap kept between the widget and the screen edges by the default position.
pub const DEFAULT_MARGIN: f64 = 16.0;

/// True when the grip strip (the top `grip_h` rows of `window`) is fully inside
/// one monitor's work area horizontally for at least `min_w` columns. A grip
/// split across two monitors does not count: it must be grabbable on one.
pub fn grip_reachable(window: Rect, monitors: &[Rect], grip_h: u32, min_w: u32) -> bool {
    let grip = Rect::new(window.x, window.y, window.w, grip_h.min(window.h));
    let need_w = (min_w as i64).min(window.w as i64);
    monitors.iter().any(|m| {
        let (w, h) = grip.overlap(m);
        h >= grip.h as i64 && w >= need_w
    })
}

/// Top-left of a widget of `window_w` physical pixels, placed at the top-right
/// of `work_area` with `margin` pixels of space.
pub fn default_position(work_area: Rect, window_w: u32, margin: u32) -> (i32, i32) {
    let x = work_area.right() - window_w as i64 - margin as i64;
    let x = x.max(work_area.x as i64);
    (x as i32, work_area.y.saturating_add(margin as i32))
}

/// Where "Reset position" puts the widget, in the target monitor's own pixel
/// space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResetTarget {
    /// Top-left in physical pixels (the target monitor's scale).
    pub physical: (i32, i32),
    /// The same point in logical units (physical / monitor scale).
    pub logical: (f64, f64),
}

/// Compute the reset position for a monitor whose work area is `work_area`
/// (physical pixels at `monitor_scale`). The window is `window_w` physical
/// pixels wide at `window_scale` (the scale of the monitor it is on now), so its
/// logical width is converted to the target monitor before placing it.
pub fn reset_target(
    work_area: Rect,
    monitor_scale: f64,
    window_w: u32,
    window_scale: f64,
    margin_logical: f64,
) -> ResetTarget {
    let logical_w = window_w as f64 / window_scale;
    let target_w = (logical_w * monitor_scale).round() as u32;
    let margin = (margin_logical * monitor_scale).round() as u32;
    let (x, y) = default_position(work_area, target_w, margin);
    ResetTarget {
        physical: (x, y),
        logical: (x as f64 / monitor_scale, y as f64 / monitor_scale),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRIP_H: u32 = 30;
    const MIN_W: u32 = 60;
    const WIN_W: u32 = 260;
    const WIN_H: u32 = 150;

    fn reachable(x: i32, y: i32, monitors: &[Rect]) -> bool {
        grip_reachable(Rect::new(x, y, WIN_W, WIN_H), monitors, GRIP_H, MIN_W)
    }

    fn primary() -> Rect {
        Rect::new(0, 0, 1920, 1040)
    }

    #[test]
    fn fully_inside_is_reachable() {
        assert!(reachable(100, 100, &[primary()]));
    }

    #[test]
    fn no_monitors_is_not_reachable() {
        assert!(!reachable(100, 100, &[]));
    }

    #[test]
    fn far_off_screen_is_not_reachable() {
        assert!(!reachable(5000, 100, &[primary()]));
        assert!(!reachable(100, 3000, &[primary()]));
        assert!(!reachable(-1000, -1000, &[primary()]));
    }

    #[test]
    fn hanging_off_right_edge() {
        // 60 px visible is enough, 59 is not.
        assert!(reachable(1920 - 60, 100, &[primary()]));
        assert!(!reachable(1920 - 59, 100, &[primary()]));
    }

    #[test]
    fn hanging_off_left_edge() {
        assert!(reachable(-(WIN_W as i32) + 60, 100, &[primary()]));
        assert!(!reachable(-(WIN_W as i32) + 59, 100, &[primary()]));
    }

    #[test]
    fn hanging_off_bottom_edge_needs_whole_grip_height() {
        assert!(reachable(100, 1040 - 30, &[primary()]));
        assert!(!reachable(100, 1040 - 29, &[primary()]));
    }

    #[test]
    fn hanging_off_top_edge_hides_the_grip() {
        assert!(reachable(100, 0, &[primary()]));
        assert!(!reachable(100, -1, &[primary()]));
        // Only the body is on screen: the grip is above the work area.
        assert!(!reachable(100, -100, &[primary()]));
    }

    #[test]
    fn work_area_excludes_taskbar() {
        // Taskbar occupies the bottom 40 px; the work area stops at 1040.
        assert!(!reachable(100, 1050, &[primary()]));
    }

    #[test]
    fn monitor_to_the_left_with_negative_coordinates() {
        let left = Rect::new(-1920, 0, 1920, 1040);
        let monitors = [left, primary()];
        assert!(reachable(-1500, 200, &monitors));
        // Entirely on the left monitor, but only 60 px from its left edge cut off.
        assert!(reachable(-1920 - 200, 200, &monitors));
        assert!(!reachable(-1920 - 201, 200, &[left]));
    }

    #[test]
    fn monitor_above_with_negative_y() {
        let above = Rect::new(0, -1080, 1920, 1080);
        let monitors = [above, primary()];
        assert!(reachable(300, -500, &monitors));
        // Grip straddling the boundary between the two stacked monitors: each
        // monitor has only part of the grip height, so it is not grabbable.
        assert!(!reachable(300, -15, &[Rect::new(0, -1080, 1920, 1065), primary()]));
    }

    #[test]
    fn grip_split_across_two_monitors_counts_only_if_one_holds_it() {
        let a = Rect::new(0, 0, 1000, 800);
        let b = Rect::new(1000, 0, 1000, 800);
        // 130 px on each side: the wide-enough part lies on one monitor.
        assert!(reachable(870, 100, &[a, b]));
        // Window not on either monitor's rows.
        assert!(!reachable(870, 900, &[a, b]));
    }

    #[test]
    fn gap_between_monitors_is_not_reachable() {
        let a = Rect::new(0, 0, 1000, 800);
        let b = Rect::new(1500, 0, 1000, 800);
        assert!(!reachable(1100, 100, &[a, b]));
        // 40 px on a, nothing on b.
        assert!(!reachable(960, 100, &[a, b]));
        // 10 px on b.
        assert!(!reachable(1250, 100, &[a, b]));
        // 60 px on b.
        assert!(reachable(1250 + 50, 100, &[a, b]));
    }

    #[test]
    fn narrow_window_needs_only_its_own_width() {
        let narrow = Rect::new(10, 10, 40, 100);
        assert!(grip_reachable(narrow, &[primary()], GRIP_H, MIN_W));
    }

    #[test]
    fn scaled_pixels_are_the_callers_business() {
        // At 200% scale: 60 px grip, 120 px min width, 520 px window.
        let win = Rect::new(1920 - 120, 100, 520, 300);
        assert!(grip_reachable(win, &[primary()], 60, 120));
        let win = Rect::new(1920 - 119, 100, 520, 300);
        assert!(!grip_reachable(win, &[primary()], 60, 120));
    }

    #[test]
    fn default_position_is_top_right_with_margin() {
        assert_eq!(default_position(primary(), 260, 16), (1920 - 260 - 16, 16));
    }

    #[test]
    fn default_position_respects_work_area_origin() {
        // Secondary monitor with negative x and a menu bar / taskbar at the top.
        let wa = Rect::new(-1920, 25, 1920, 1055);
        assert_eq!(default_position(wa, 260, 16), (-260 - 16, 25 + 16));
    }

    #[test]
    fn default_position_never_left_of_the_work_area() {
        let tiny = Rect::new(100, 0, 200, 200);
        assert_eq!(default_position(tiny, 260, 16).0, 100);
    }

    #[test]
    fn default_position_makes_the_grip_reachable() {
        let wa = Rect::new(-1920, 25, 1920, 1055);
        let (x, y) = default_position(wa, WIN_W, 16);
        assert!(grip_reachable(Rect::new(x, y, WIN_W, WIN_H), &[wa], GRIP_H, MIN_W));
    }

    #[test]
    fn reset_target_same_scale() {
        let t = reset_target(primary(), 1.0, 260, 1.0, 16.0);
        assert_eq!(t.physical, (1920 - 260 - 16, 16));
        assert_eq!(t.logical, (1644.0, 16.0));
    }

    #[test]
    fn reset_target_window_on_1x_monitor_primary_is_2x() {
        // Window is 260 px wide at 1x; primary is 2x (3840x2160 physical).
        let wa = Rect::new(0, 0, 3840, 2080);
        let t = reset_target(wa, 2.0, 260, 1.0, 16.0);
        // Target width 520, margin 32.
        assert_eq!(t.physical, (3840 - 520 - 32, 32));
        assert_eq!(t.logical, ((3840.0 - 552.0) / 2.0, 16.0));
    }

    #[test]
    fn reset_target_window_on_2x_monitor_primary_is_1x() {
        // Window is 520 px wide at 2x; primary is 1x.
        let t = reset_target(primary(), 1.0, 520, 2.0, 16.0);
        assert_eq!(t.physical, (1920 - 260 - 16, 16));
        assert_eq!(t.logical, (1644.0, 16.0));
    }

    #[test]
    fn reset_target_fractional_primary_scale() {
        // 1.5x primary 2880x1620, window 260 px wide at 1x: 390 px, margin 24.
        let wa = Rect::new(0, 0, 2880, 1560);
        let t = reset_target(wa, 1.5, 260, 1.0, 16.0);
        assert_eq!(t.physical, (2880 - 390 - 24, 24));
        assert_eq!(t.logical, ((2880.0 - 414.0) / 1.5, 16.0));
    }

    #[test]
    fn reset_target_primary_with_non_zero_origin() {
        // Primary is not at 0,0 (e.g. arranged right of another screen), 2x.
        let wa = Rect::new(-3840, 50, 3840, 2100);
        let t = reset_target(wa, 2.0, 520, 2.0, 16.0);
        assert_eq!(t.physical, (-3840 + 3840 - 520 - 32, 50 + 32));
        assert_eq!(t.logical, (-276.0, 41.0));
    }

    #[test]
    fn reset_target_keeps_grip_reachable_on_target() {
        let wa = Rect::new(0, 0, 3840, 2080);
        let t = reset_target(wa, 2.0, 260, 1.0, 16.0);
        assert!(grip_reachable(Rect::new(t.physical.0, t.physical.1, 520, 300), &[wa], 60, 120));
    }
}
