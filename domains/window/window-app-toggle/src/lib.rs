pub mod config;

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use config::AppConfig;
use window_core::WinId;

const LAUNCH_RETRY: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppKind {
    WezTerm,
    DoubleCommander,
}

pub type AppToggleAction = AppKind;

impl AppKind {
    const ALL: [Self; 2] = [Self::WezTerm, Self::DoubleCommander];

    fn index(self) -> usize {
        match self {
            Self::WezTerm => 0,
            Self::DoubleCommander => 1,
        }
    }
}

pub trait AppTogglePlatform {
    fn foreground(&self) -> Option<WinId>;
    fn last_active(&self) -> Option<WinId>;
    fn is_zonable(&self, id: WinId) -> bool;
    fn app_windows(&self, app: &AppConfig) -> Vec<WinId>;
    fn focus(&mut self, id: WinId) -> Result<()>;
    fn minimize(&mut self, id: WinId) -> Result<()>;

    fn launch(&mut self, app: &AppConfig) -> Result<()> {
        std::process::Command::new(&app.command)
            .args(&app.args)
            .spawn()
            .with_context(|| format!("cannot launch {}", app.command.display()))?;
        Ok(())
    }
}

#[derive(Default)]
struct AppState {
    previous: Option<WinId>,
    launching: Option<Instant>,
}

/// Per-application focus state. Process/window discovery stays behind the
/// platform trait so every transition is testable without launching apps.
#[derive(Default)]
pub struct AppToggle {
    states: [AppState; AppKind::ALL.len()],
}

impl AppToggle {
    pub fn toggle<P: AppTogglePlatform>(
        &mut self,
        platform: &mut P,
        kind: AppKind,
        app: &AppConfig,
    ) -> Result<()> {
        self.toggle_at(platform, kind, app, Instant::now())
    }

    fn toggle_at<P: AppTogglePlatform>(
        &mut self,
        platform: &mut P,
        kind: AppKind,
        app: &AppConfig,
        now: Instant,
    ) -> Result<()> {
        let state = &mut self.states[kind.index()];
        let windows = platform.app_windows(app);
        let foreground = platform.foreground();
        if windows.is_empty() {
            state.previous = foreground.filter(|id| platform.is_zonable(*id));
            if state
                .launching
                .is_some_and(|at| now.duration_since(at) < LAUNCH_RETRY)
            {
                return Ok(());
            }
            platform.launch(app)?;
            state.launching = Some(now);
            return Ok(());
        }
        state.launching = None;

        if let Some(current) = foreground.filter(|id| windows.contains(id)) {
            let return_to = state
                .previous
                .filter(|id| platform.is_zonable(*id) && !windows.contains(id))
                .or_else(|| {
                    platform
                        .last_active()
                        .filter(|id| platform.is_zonable(*id) && !windows.contains(id))
                });
            if let Some(previous) = return_to {
                platform.focus(previous)?;
            }
            platform.minimize(current)?;
            state.previous = return_to;
            return Ok(());
        }

        let target = state
            .previous
            .filter(|id| windows.contains(id))
            .unwrap_or(windows[0]);
        if let Some(current) = foreground.filter(|id| platform.is_zonable(*id)) {
            state.previous = Some(current);
        }
        platform.focus(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Mock {
        foreground: Option<WinId>,
        last_active: Option<WinId>,
        visible: Vec<WinId>,
        windows: Vec<WinId>,
        events: Vec<String>,
        launches: usize,
    }

    impl Mock {
        fn show(&mut self, foreground: WinId, windows: &[u64], others: &[u64]) {
            self.foreground = Some(foreground);
            self.windows = windows.to_vec();
            self.visible = windows.iter().chain(others).copied().collect();
        }
    }

    impl AppTogglePlatform for Mock {
        fn foreground(&self) -> Option<WinId> {
            self.foreground
        }
        fn last_active(&self) -> Option<WinId> {
            self.last_active
        }
        fn is_zonable(&self, id: WinId) -> bool {
            self.visible.contains(&id)
        }
        fn app_windows(&self, _app: &AppConfig) -> Vec<WinId> {
            self.windows.clone()
        }
        fn focus(&mut self, id: WinId) -> Result<()> {
            self.foreground = Some(id);
            self.events.push(format!("focus:{}", id));
            Ok(())
        }
        fn minimize(&mut self, id: WinId) -> Result<()> {
            self.events.push(format!("minimize:{}", id));
            Ok(())
        }
        fn launch(&mut self, _app: &AppConfig) -> Result<()> {
            self.launches += 1;
            self.events.push("launch".into());
            Ok(())
        }
    }

    fn app() -> AppConfig {
        config::Config::default().wezterm
    }

    #[test]
    fn launches_once_then_focuses_the_new_window() {
        let now = Instant::now();
        let mut mock = Mock::default();
        mock.show(20, &[], &[20]);
        let mut toggle = AppToggle::default();
        toggle
            .toggle_at(&mut mock, AppKind::WezTerm, &app(), now)
            .unwrap();
        toggle
            .toggle_at(
                &mut mock,
                AppKind::WezTerm,
                &app(),
                now + Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(mock.launches, 1);
        mock.show(20, &[10], &[20]);
        toggle
            .toggle_at(
                &mut mock,
                AppKind::WezTerm,
                &app(),
                now + Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(mock.foreground, Some(10));
        assert_eq!(mock.events, ["launch", "focus:10"]);
    }

    #[test]
    fn single_window_returns_to_the_previous_application() {
        let mut mock = Mock::default();
        mock.show(20, &[10], &[20]);
        let mut toggle = AppToggle::default();
        toggle.toggle(&mut mock, AppKind::WezTerm, &app()).unwrap();
        toggle.toggle(&mut mock, AppKind::WezTerm, &app()).unwrap();
        assert_eq!(mock.events, ["focus:10", "focus:20", "minimize:10"]);
    }

    #[test]
    fn multiple_windows_use_topmost_and_never_cycle() {
        let mut mock = Mock::default();
        mock.show(20, &[10, 11], &[20]);
        let mut toggle = AppToggle::default();
        toggle.toggle(&mut mock, AppKind::WezTerm, &app()).unwrap();
        assert_eq!(mock.events, ["focus:10"]);

        mock.show(10, &[10, 11], &[20]);
        toggle.toggle(&mut mock, AppKind::WezTerm, &app()).unwrap();
        assert_eq!(mock.events, ["focus:10", "focus:20", "minimize:10"]);

        mock.show(20, &[11, 10], &[20]);
        toggle.toggle(&mut mock, AppKind::WezTerm, &app()).unwrap();
        assert_eq!(
            mock.events,
            ["focus:10", "focus:20", "minimize:10", "focus:11"]
        );
    }

    #[test]
    fn closed_previous_window_is_not_reused() {
        let mut mock = Mock::default();
        mock.show(10, &[10], &[]);
        let mut toggle = AppToggle::default();
        toggle.states[0].previous = Some(99);
        toggle.toggle(&mut mock, AppKind::WezTerm, &app()).unwrap();
        assert_eq!(mock.events, ["minimize:10"]);
        assert_eq!(toggle.states[0].previous, None);
    }

    #[test]
    fn reopening_after_all_windows_close_starts_clean() {
        let now = Instant::now();
        let mut mock = Mock::default();
        mock.show(20, &[], &[20]);
        let mut toggle = AppToggle::default();
        toggle.states[0].previous = Some(10);
        toggle
            .toggle_at(&mut mock, AppKind::WezTerm, &app(), now)
            .unwrap();
        assert_eq!(toggle.states[0].previous, Some(20));
        toggle
            .toggle_at(&mut mock, AppKind::WezTerm, &app(), now + LAUNCH_RETRY)
            .unwrap();
        assert_eq!(mock.launches, 2);
    }
}
