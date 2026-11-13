//! Composition-root platform interface and feature dispatch.
//!
//! Feature state machines live in their own crates; these adapters expose only
//! the narrow operations each feature needs without changing the OS backends.

use crate::config::Action;
use crate::layout::{Rect, SizeSpec};
use anyhow::Result;
use std::path::Path;
use window_app_switch::{Switcher, SwitcherPlatform};
use window_app_toggle::{AppKind, AppToggle, AppTogglePlatform};
use window_maximize_minimize::{MaximizeMinimize, MaximizeMinimizeAction};
use window_on_top_bottom::{OnTopBottom, OnTopBottomAction};
use window_rectangle::{Rectangle, RectangleAction};
use window_resize::Resize;

use crate::history::History;
pub use window_core::{Monitor, WinId};
use window_core::{WindowOps, WindowQuery};
pub use window_rectangle::SizerPick;

pub trait Platform {
    /// Work areas of all monitors, in enumeration order.
    fn monitors(&self) -> Vec<Monitor>;
    /// Index into `monitors()` for the monitor nearest `id`.
    fn monitor_index(&self, id: WinId) -> usize;
    fn foreground(&self) -> Option<WinId>;
    /// Last zonable foreground window remembered by a poll thread (Windows).
    fn last_active(&self) -> Option<WinId> {
        None
    }
    fn is_zonable(&self, id: WinId) -> bool;
    /// Full window rect, including invisible resize borders.
    fn window_rect(&self, id: WinId) -> Option<Rect>;
    /// Visible frame rect (DWM extended frame bounds on Windows).
    fn visible_frame(&self, id: WinId) -> Option<Rect>;
    fn move_window(&self, id: WinId, r: Rect) -> Result<()>;
    fn is_maximized(&self, id: WinId) -> bool;
    fn set_maximized(&self, id: WinId, on: bool) -> Result<()>;
    fn is_always_on_top(&self, id: WinId) -> bool;
    fn set_always_on_top(&self, id: WinId, on: bool) -> Result<()>;
    /// Windows re-asserts always-on-bottom on its bottomReassertMs timer.
    fn is_always_on_bottom(&self, _id: WinId) -> bool {
        false
    }
    fn set_always_on_bottom(&self, _id: WinId, _on: bool) -> Result<()> {
        anyhow::bail!("always-on-bottom is not supported on this platform")
    }
    fn minimize(&self, id: WinId) -> Result<()>;
    fn restore(&self, id: WinId) -> Result<()>;
    fn unpin_all(&self) -> Result<()>;
    fn same_app_windows(&self, id: WinId) -> Vec<WinId> {
        vec![id]
    }
    /// Visible windows belonging to this exact executable path.
    fn app_windows(&self, _exe: &Path) -> Vec<WinId> {
        Vec::new()
    }
    fn focus(&self, id: WinId) -> Result<()>;

    fn switch_desktop(&self, _n: u8) -> Result<()> {
        anyhow::bail!("virtual desktops are not supported on this platform")
    }
    fn move_to_desktop(&self, _id: WinId, _n: u8) -> Result<()> {
        anyhow::bail!("virtual desktops are not supported on this platform")
    }
    fn next_desktop(&self) -> Result<()> {
        anyhow::bail!("virtual desktops are not supported on this platform")
    }
    fn prev_desktop(&self) -> Result<()> {
        anyhow::bail!("virtual desktops are not supported on this platform")
    }
    fn scale(&self) -> Result<()> {
        anyhow::bail!("window zoom is not supported on this platform")
    }
    fn file_dialog(&self) -> Result<()> {
        anyhow::bail!("file dialogs are not supported on this platform")
    }
    fn pick_sizer(&self, _presets: &[SizeSpec]) -> SizerPick {
        SizerPick::Unsupported
    }
    fn find_by_pid(&self, _pid: u32) -> Option<WinId> {
        None
    }
}

struct SwitcherBackend<'a, P>(&'a P);

impl<P: Platform> WindowQuery for SwitcherBackend<'_, P> {
    fn foreground(&self) -> Option<WinId> {
        self.0.foreground()
    }
    fn last_active(&self) -> Option<WinId> {
        self.0.last_active()
    }
    fn is_zonable(&self, id: WinId) -> bool {
        self.0.is_zonable(id)
    }
}

impl<P: Platform> SwitcherPlatform for SwitcherBackend<'_, P> {
    fn same_app_windows(&self, id: WinId) -> Vec<WinId> {
        self.0.same_app_windows(id)
    }
    fn focus(&self, id: WinId) -> Result<()> {
        self.0.focus(id)
    }
}

struct WindowBackend<'a, P>(&'a P);

impl<P: Platform> WindowQuery for WindowBackend<'_, P> {
    fn foreground(&self) -> Option<WinId> {
        self.0.foreground()
    }
    fn last_active(&self) -> Option<WinId> {
        self.0.last_active()
    }
    fn is_zonable(&self, id: WinId) -> bool {
        self.0.is_zonable(id)
    }
}

impl<P: Platform> WindowOps for WindowBackend<'_, P> {
    fn monitors(&self) -> Vec<Monitor> {
        self.0.monitors()
    }
    fn monitor_index(&self, id: WinId) -> usize {
        self.0.monitor_index(id)
    }
    fn window_rect(&self, id: WinId) -> Option<Rect> {
        self.0.window_rect(id)
    }
    fn visible_frame(&self, id: WinId) -> Option<Rect> {
        self.0.visible_frame(id)
    }
    fn move_window(&self, id: WinId, r: Rect) -> Result<()> {
        self.0.move_window(id, r)
    }
    fn is_maximized(&self, id: WinId) -> bool {
        self.0.is_maximized(id)
    }
    fn set_maximized(&self, id: WinId, on: bool) -> Result<()> {
        self.0.set_maximized(id, on)
    }
    fn is_always_on_top(&self, id: WinId) -> bool {
        self.0.is_always_on_top(id)
    }
    fn set_always_on_top(&self, id: WinId, on: bool) -> Result<()> {
        self.0.set_always_on_top(id, on)
    }
    fn is_always_on_bottom(&self, id: WinId) -> bool {
        self.0.is_always_on_bottom(id)
    }
    fn set_always_on_bottom(&self, id: WinId, on: bool) -> Result<()> {
        self.0.set_always_on_bottom(id, on)
    }
    fn minimize(&self, id: WinId) -> Result<()> {
        self.0.minimize(id)
    }
    fn restore(&self, id: WinId) -> Result<()> {
        self.0.restore(id)
    }
    fn unpin_all(&self) -> Result<()> {
        self.0.unpin_all()
    }
    fn pick_sizer(&self, presets: &[SizeSpec]) -> SizerPick {
        self.0.pick_sizer(presets)
    }
}

struct AppToggleBackend<'a, P>(&'a mut P);

impl<P: Platform> AppTogglePlatform for AppToggleBackend<'_, P> {
    fn foreground(&self) -> Option<WinId> {
        self.0.foreground()
    }
    fn last_active(&self) -> Option<WinId> {
        self.0.last_active()
    }
    fn is_zonable(&self, id: WinId) -> bool {
        self.0.is_zonable(id)
    }
    fn app_windows(&self, app: &window_app_toggle::config::AppConfig) -> Vec<WinId> {
        self.0.app_windows(&app.exe)
    }
    fn focus(&mut self, id: WinId) -> Result<()> {
        self.0.focus(id)
    }
    fn minimize(&mut self, id: WinId) -> Result<()> {
        self.0.minimize(id)
    }
}

pub struct App<P: Platform> {
    pub p: P,
    rectangle: Rectangle,
    resize: Resize,
    pins: OnTopBottom,
    window_state: MaximizeMinimize,
    switcher: Switcher,
    app_toggle: AppToggle,
    app_toggle_config: window_app_toggle::config::Config,
    history: History,
}

impl<P: Platform> App<P> {
    pub fn new(p: P) -> Self {
        Self {
            p,
            rectangle: Rectangle::default(),
            resize: Resize::default(),
            pins: OnTopBottom,
            window_state: MaximizeMinimize,
            switcher: Switcher::default(),
            app_toggle: AppToggle::default(),
            app_toggle_config: window_app_toggle::config::Config::default(),
            history: History::default(),
        }
    }

    pub fn set_gap(&mut self, gap: i32) {
        self.rectangle.set_gap(gap);
    }

    pub fn set_sizer(&mut self, presets: Vec<SizeSpec>) {
        self.resize.set_sizer(presets);
    }

    pub fn set_app_toggle(&mut self, config: window_app_toggle::config::Config) {
        self.app_toggle_config = config;
    }

    pub fn apply(&mut self, action: Action) -> Result<()> {
        self.apply_to(action, None)
    }

    /// `forced` overrides the foreground target (for `--pid <pid>` one-shots).
    pub fn apply_to(&mut self, action: Action, forced: Option<WinId>) -> Result<()> {
        use RectangleAction as R;
        let rectangle = match action {
            Action::MoveToLeft => Some(R::MoveToLeft),
            Action::MoveToRight => Some(R::MoveToRight),
            Action::MoveToTop => Some(R::MoveToTop),
            Action::MoveToBottom => Some(R::MoveToBottom),
            Action::MoveToTopLeft => Some(R::MoveToTopLeft),
            Action::MoveToTopRight => Some(R::MoveToTopRight),
            Action::MoveToBottomLeft => Some(R::MoveToBottomLeft),
            Action::MoveToBottomRight => Some(R::MoveToBottomRight),
            Action::MoveToCenter => Some(R::MoveToCenter),
            Action::NextDisplay => Some(R::NextDisplay),
            Action::PrevDisplay => Some(R::PrevDisplay),
            _ => None,
        };
        if let Some(action) = rectangle {
            return self.rectangle.apply(
                &WindowBackend(&self.p),
                &mut self.history,
                action,
                forced,
            );
        }
        match action {
            Action::NextWindow => self.switcher.switch(&SwitcherBackend(&self.p), 1, forced),
            Action::PrevWindow => self.switcher.switch(&SwitcherBackend(&self.p), -1, forced),
            Action::SizerMenu => {
                self.rectangle.reset_cycle();
                self.resize
                    .apply(&WindowBackend(&self.p), &mut self.history, forced)
            }
            Action::Scale => self.p.scale(),
            Action::ToggleWezTerm => self.app_toggle.toggle(
                &mut AppToggleBackend(&mut self.p),
                AppKind::WezTerm,
                &self.app_toggle_config.wezterm,
            ),
            Action::ToggleDoubleCommander => self.app_toggle.toggle(
                &mut AppToggleBackend(&mut self.p),
                AppKind::DoubleCommander,
                &self.app_toggle_config.double_commander,
            ),
            Action::ToggleAlwaysOnTop => self.pins.apply(
                &WindowBackend(&self.p),
                OnTopBottomAction::ToggleAlwaysOnTop,
                forced,
            ),
            Action::ToggleAlwaysOnBottom => self.pins.apply(
                &WindowBackend(&self.p),
                OnTopBottomAction::ToggleAlwaysOnBottom,
                forced,
            ),
            Action::UnpinAll => self.pins.apply(
                &WindowBackend(&self.p),
                OnTopBottomAction::ClearAllPins,
                forced,
            ),
            Action::Maximize => {
                self.rectangle.reset_cycle();
                self.window_state.apply(
                    &WindowBackend(&self.p),
                    &mut self.history,
                    MaximizeMinimizeAction::Maximize,
                    forced,
                )
            }
            Action::Minimize => {
                self.rectangle.reset_cycle();
                self.window_state.apply(
                    &WindowBackend(&self.p),
                    &mut self.history,
                    MaximizeMinimizeAction::Minimize,
                    forced,
                )
            }
            Action::Undo => {
                self.rectangle.reset_cycle();
                self.history.undo(&WindowBackend(&self.p))
            }
            Action::Quit | Action::Suppress => Ok(()),
            Action::FileDialog => self.p.file_dialog(),
            Action::SwitchToDesktop(n) => self.p.switch_desktop(n),
            Action::NextDesktop => self.p.next_desktop(),
            Action::PrevDesktop => self.p.prev_desktop(),
            Action::MoveToDesktop(n) => {
                let t = window_core::target(&WindowBackend(&self.p), forced)?;
                self.p.move_to_desktop(t, n)
            }
            _ => unreachable!("rectangle actions returned above"),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory platform: one 1000x700 work area at (0,0), a zonable window
    /// whose rect updates on move so cycle progression is observable.
    struct Mock {
        rect: std::cell::RefCell<Rect>,
        monitors: Vec<Monitor>,
        maximized: std::cell::Cell<bool>,
        minimized: std::cell::RefCell<std::collections::HashSet<WinId>>,
        aot: std::cell::Cell<bool>,
        foreground: std::cell::Cell<WinId>,
        focused: std::cell::Cell<WinId>,
        app_windows: std::cell::RefCell<Vec<WinId>>,
    }

    impl Platform for Mock {
        fn monitors(&self) -> Vec<Monitor> {
            self.monitors.clone()
        }
        fn monitor_index(&self, _id: WinId) -> usize {
            0
        }
        fn foreground(&self) -> Option<WinId> {
            let id = self.foreground.get();
            (id != 0).then_some(id)
        }
        fn is_zonable(&self, _id: WinId) -> bool {
            true
        }
        fn window_rect(&self, _id: WinId) -> Option<Rect> {
            Some(*self.rect.borrow())
        }
        fn visible_frame(&self, _id: WinId) -> Option<Rect> {
            Some(*self.rect.borrow())
        }
        fn move_window(&self, _id: WinId, r: Rect) -> Result<()> {
            *self.rect.borrow_mut() = r;
            Ok(())
        }
        fn is_maximized(&self, _id: WinId) -> bool {
            self.maximized.get()
        }
        fn set_maximized(&self, _id: WinId, on: bool) -> Result<()> {
            self.maximized.set(on);
            Ok(())
        }
        fn is_always_on_top(&self, _id: WinId) -> bool {
            self.aot.get()
        }
        fn set_always_on_top(&self, _id: WinId, on: bool) -> Result<()> {
            self.aot.set(on);
            Ok(())
        }
        fn minimize(&self, id: WinId) -> Result<()> {
            self.minimized.borrow_mut().insert(id);
            Ok(())
        }
        fn restore(&self, id: WinId) -> Result<()> {
            self.minimized.borrow_mut().remove(&id);
            Ok(())
        }
        fn unpin_all(&self) -> Result<()> {
            self.aot.set(false);
            Ok(())
        }
        fn same_app_windows(&self, _id: WinId) -> Vec<WinId> {
            self.app_windows.borrow().clone()
        }
        fn focus(&self, id: WinId) -> Result<()> {
            self.focused.set(id);
            self.foreground.set(id);
            Ok(())
        }
    }

    fn mock() -> Mock {
        Mock {
            rect: std::cell::RefCell::new(Rect {
                l: 10,
                t: 10,
                r: 610,
                b: 410,
            }),
            monitors: vec![Monitor {
                work: Rect {
                    l: 0,
                    t: 0,
                    r: 1000,
                    b: 700,
                },
            }],
            maximized: std::cell::Cell::new(false),
            minimized: std::cell::RefCell::new(std::collections::HashSet::new()),
            aot: std::cell::Cell::new(false),
            foreground: std::cell::Cell::new(1),
            focused: std::cell::Cell::new(1),
            app_windows: std::cell::RefCell::new(vec![1]),
        }
    }

    #[test]
    fn cycle_progression_and_wrap() {
        let mut app = App::new(mock());
        // Same window, repeated presses: 1/2 -> 2/3 -> 1/3 -> back to 1/2.
        for expected in [500, 667, 334, 500] {
            app.apply(Action::MoveToLeft).unwrap();
            assert_eq!(app.p.window_rect(1).unwrap().w(), expected, "cycle width");
        }
        // A different direction resets the left counter: right = half again.
        app.apply(Action::MoveToRight).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap().r, 1000);
        app.apply(Action::MoveToLeft).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap().w(), 500);
    }

    #[test]
    fn changing_between_edges_and_corners_restarts_at_half() {
        let mut app = App::new(mock());
        app.apply(Action::MoveToLeft).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap().w(), 500);

        app.apply(Action::MoveToBottomRight).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap().w(), 500);
        app.apply(Action::MoveToBottomRight).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap().w(), 667);

        app.apply(Action::MoveToLeft).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap().w(), 500);
    }

    #[test]
    fn corner_cycles_horizontal_only() {
        let mut app = App::new(mock());
        app.apply(Action::MoveToTopLeft).unwrap();
        assert_eq!(
            app.p.window_rect(1).unwrap(),
            Rect {
                l: 0,
                t: 0,
                r: 500,
                b: 350
            }
        );
        app.apply(Action::MoveToTopLeft).unwrap(); // two thirds wide
        assert_eq!(app.p.window_rect(1).unwrap().r, 667);
    }

    #[test]
    fn maximize_toggles() {
        let mut app = App::new(mock());
        app.apply(Action::Maximize).unwrap();
        assert!(app.p.is_maximized(1));
        app.apply(Action::Maximize).unwrap();
        assert!(!app.p.is_maximized(1));
    }

    #[test]
    fn minimize_minimizes_the_target() {
        let mut app = App::new(mock());
        app.apply(Action::Minimize).unwrap();
        assert!(app.p.minimized.borrow().contains(&1));
    }

    #[test]
    fn unpin_all_clears_topmost() {
        let mut app = App::new(mock());
        app.p.aot.set(true);
        app.apply(Action::UnpinAll).unwrap();
        assert!(!app.p.aot.get());
    }

    #[test]
    fn next_window_cycles_the_foreground_app() {
        let mut app = App::new(mock());
        *app.p.app_windows.borrow_mut() = vec![1, 2, 3];

        app.apply(Action::NextWindow).unwrap();
        assert_eq!(app.p.focused.get(), 2);
        app.apply(Action::NextWindow).unwrap();
        assert_eq!(app.p.focused.get(), 3);
        app.apply(Action::NextWindow).unwrap();
        assert_eq!(app.p.focused.get(), 1);
        app.apply(Action::PrevWindow).unwrap();
        assert_eq!(app.p.focused.get(), 3);

        // Another app starts a fresh cycle instead of reusing the old list.
        app.p.foreground.set(9);
        *app.p.app_windows.borrow_mut() = vec![9, 10];
        app.apply(Action::PrevWindow).unwrap();
        assert_eq!(app.p.focused.get(), 10);
    }

    #[test]
    fn sizer_cycles_presets_without_a_popup() {
        let mut app = App::new(mock());
        app.set_sizer(vec!["500x300".parse().unwrap(), "1/2".parse().unwrap()]);

        app.apply(Action::SizerMenu).unwrap();
        assert_eq!(
            app.p.window_rect(1).unwrap(),
            Rect {
                l: 10,
                t: 10,
                r: 510,
                b: 310
            }
        );
        app.apply(Action::SizerMenu).unwrap();
        assert_eq!(
            app.p.window_rect(1).unwrap(),
            Rect {
                l: 10,
                t: 10,
                r: 510,
                b: 360
            }
        );
        // Past the last preset: wrap to the first.
        app.apply(Action::SizerMenu).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap().h(), 300);
    }

    #[test]
    fn next_display_scales_to_the_target_monitor() {
        let mut app = App::new(mock());
        app.p.monitors.push(Monitor {
            work: Rect {
                l: 1000,
                t: 0,
                r: 3000,
                b: 1050,
            },
        });
        app.apply(Action::NextDisplay).unwrap();
        // 600x400 window at the 1000x700 ratio, scaled 1.5x and centered.
        assert_eq!(
            app.p.window_rect(1).unwrap(),
            Rect {
                l: 1550,
                t: 225,
                r: 2450,
                b: 825
            }
        );
    }

    #[test]
    fn other_resize_actions_restart_the_snap_cycle() {
        for interrupt in [
            Action::MoveToCenter,
            Action::NextDisplay,
            Action::SizerMenu,
            Action::Maximize,
            Action::Minimize,
        ] {
            let mut app = App::new(mock());
            app.set_sizer(vec![crate::layout::SizeSpec::Px { w: 800, h: 600 }]);
            app.apply(Action::MoveToLeft).unwrap();
            app.apply(Action::MoveToLeft).unwrap();
            assert_eq!(app.p.window_rect(1).unwrap().w(), 667);
            app.apply(interrupt).unwrap();
            app.apply(Action::MoveToLeft).unwrap();
            assert_eq!(
                app.p.window_rect(1).unwrap().w(),
                500,
                "after {interrupt:?}"
            );
        }
    }

    #[test]
    fn undo_walks_back_the_history() {
        let mut app = App::new(mock());
        let before = app.p.window_rect(1).unwrap();

        app.apply(Action::MoveToLeft).unwrap();
        let half = app.p.window_rect(1).unwrap();
        app.apply(Action::MoveToLeft).unwrap();
        let two_thirds = app.p.window_rect(1).unwrap();
        assert_eq!(two_thirds.w(), 667);

        // Multi-level: each undo restores the previous step.
        app.apply(Action::Undo).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap(), half);
        app.apply(Action::Undo).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap(), before);

        // History exhausted: a further undo reports it instead of a silent
        // no-op, so the hotkey never looks dead.
        assert!(app.apply(Action::Undo).is_err());
        assert_eq!(app.p.window_rect(1).unwrap(), before);

        // Undo reset the cycle counter, so left snaps to one half again.
        app.apply(Action::MoveToLeft).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap().w(), 500);
    }

    #[test]
    fn undo_restores_minimized_windows_lifo() {
        let mut app = App::new(mock());
        app.apply_to(Action::Minimize, Some(1)).unwrap();
        app.apply_to(Action::Minimize, Some(2)).unwrap();
        assert_eq!(app.p.minimized.borrow().len(), 2);

        // Last minimized comes back first.
        app.apply(Action::Undo).unwrap();
        assert!(!app.p.minimized.borrow().contains(&2));
        assert!(app.p.minimized.borrow().contains(&1));
        app.apply(Action::Undo).unwrap();
        assert!(app.p.minimized.borrow().is_empty());
    }

    #[test]
    fn undo_snaps_and_minimize_share_one_history() {
        let mut app = App::new(mock());
        let before = app.p.window_rect(1).unwrap();
        app.apply(Action::MoveToLeft).unwrap();
        app.apply_to(Action::Minimize, Some(1)).unwrap();

        // First undo re-shows the window (still at its snapped rect),
        // second undo moves it back to the pre-snap rect.
        app.apply(Action::Undo).unwrap();
        assert!(!app.p.minimized.borrow().contains(&1));
        assert_eq!(app.p.window_rect(1).unwrap().w(), 500);
        app.apply(Action::Undo).unwrap();
        assert_eq!(app.p.window_rect(1).unwrap(), before);
    }

    #[test]
    fn gap_insets_the_work_area() {
        let mut app = App::new(mock());
        app.set_gap(20);
        app.apply(Action::MoveToLeft).unwrap();
        assert_eq!(
            app.p.window_rect(1).unwrap(),
            Rect {
                l: 20,
                t: 20,
                r: 500,
                b: 680
            }
        );
        app.apply(Action::MoveToBottomRight).unwrap();
        assert_eq!(
            app.p.window_rect(1).unwrap(),
            Rect {
                l: 500,
                t: 350,
                r: 980,
                b: 680
            }
        );
    }

    #[test]
    fn no_target_window_errors() {
        struct NoTarget;
        impl Platform for NoTarget {
            fn monitors(&self) -> Vec<Monitor> {
                vec![Monitor {
                    work: Rect {
                        l: 0,
                        t: 0,
                        r: 10,
                        b: 10,
                    },
                }]
            }
            fn monitor_index(&self, _id: WinId) -> usize {
                0
            }
            fn foreground(&self) -> Option<WinId> {
                None
            }
            fn is_zonable(&self, _id: WinId) -> bool {
                false
            }
            fn window_rect(&self, _id: WinId) -> Option<Rect> {
                None
            }
            fn visible_frame(&self, _id: WinId) -> Option<Rect> {
                None
            }
            fn move_window(&self, _id: WinId, _r: Rect) -> Result<()> {
                Ok(())
            }
            fn is_maximized(&self, _id: WinId) -> bool {
                false
            }
            fn set_maximized(&self, _id: WinId, _on: bool) -> Result<()> {
                Ok(())
            }
            fn is_always_on_top(&self, _id: WinId) -> bool {
                false
            }
            fn set_always_on_top(&self, _id: WinId, _on: bool) -> Result<()> {
                Ok(())
            }
            fn minimize(&self, _id: WinId) -> Result<()> {
                Ok(())
            }
            fn restore(&self, _id: WinId) -> Result<()> {
                Ok(())
            }
            fn unpin_all(&self) -> Result<()> {
                Ok(())
            }
            fn focus(&self, _id: WinId) -> Result<()> {
                Ok(())
            }
        }
        let mut app = App::new(NoTarget);
        assert!(app.apply(Action::MoveToLeft).is_err());
    }
}
