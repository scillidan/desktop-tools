//! Newline-delimited JSON between host and plugins. stdout is protocol-only;
//! diagnostics belong on stderr/logs.

use crate::menu::MenuModel;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 3;
pub const MAX_LINE: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum PluginEvent {
    Ready {
        v: u32,
        menu: MenuModel,
        #[serde(default)]
        status: Option<String>,
    },
    Menu {
        v: u32,
        items: MenuModel,
    },
    Status {
        v: u32,
        text: String,
    },
    /// Ask the host's tray to show a non-modal system notification.
    Notify {
        v: u32,
        title: String,
        body: String,
    },
    Error {
        v: u32,
        message: String,
    },
    Log {
        v: u32,
        level: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum HostCommand {
    Invoke { v: u32, id: String },
    Reload { v: u32 },
    Quit { v: u32 },
}

impl PluginEvent {
    pub fn to_line(&self) -> Result<String> {
        let mut line = serde_json::to_string(self).context("serialize plugin event")?;
        if line.len() > MAX_LINE {
            bail!("plugin event over {MAX_LINE} bytes");
        }
        line.push('\n');
        Ok(line)
    }

    pub fn from_line(line: &str) -> Result<PluginEvent> {
        if line.len() > MAX_LINE {
            bail!("plugin event over {MAX_LINE} bytes");
        }
        let event: PluginEvent = serde_json::from_str(line).context("parse plugin event")?;
        check_version(event.version())?;
        Ok(event)
    }

    pub fn version(&self) -> u32 {
        match self {
            PluginEvent::Ready { v, .. }
            | PluginEvent::Menu { v, .. }
            | PluginEvent::Status { v, .. }
            | PluginEvent::Notify { v, .. }
            | PluginEvent::Error { v, .. }
            | PluginEvent::Log { v, .. } => *v,
        }
    }
}

impl HostCommand {
    pub fn to_line(&self) -> Result<String> {
        let mut line = serde_json::to_string(self).context("serialize host command")?;
        if line.len() > MAX_LINE {
            bail!("host command over {MAX_LINE} bytes");
        }
        line.push('\n');
        Ok(line)
    }

    pub fn from_line(line: &str) -> Result<HostCommand> {
        if line.len() > MAX_LINE {
            bail!("host command over {MAX_LINE} bytes");
        }
        let command: HostCommand = serde_json::from_str(line).context("parse host command")?;
        let v = match &command {
            HostCommand::Invoke { v, .. } | HostCommand::Reload { v } | HostCommand::Quit { v } => {
                *v
            }
        };
        check_version(v)?;
        Ok(command)
    }
}

/// Cloneable notification sink for plugin worker threads. The shared stdout
/// lock prevents events from interleaving with the main plugin event loop.
#[cfg(windows)]
#[derive(Clone)]
pub struct PluginNotifier {
    out: std::sync::Arc<std::sync::Mutex<std::io::Stdout>>,
}

#[cfg(windows)]
impl PluginNotifier {
    pub fn notify(&self, title: impl Into<String>, body: impl Into<String>) -> Result<()> {
        send(
            &self.out,
            &PluginEvent::Notify {
                v: PROTOCOL_VERSION,
                title: title.into(),
                body: body.into(),
            },
        )
    }
}

/// Plugin-side link. The stdin reader posts `kit::tray::post_wakeup(4)` so a
/// tray or headless message loop can drain commands without blocking on I/O.
#[cfg(windows)]
pub struct PluginLink {
    out: std::sync::Arc<std::sync::Mutex<std::io::Stdout>>,
    rx: std::sync::mpsc::Receiver<HostCommand>,
}

#[cfg(windows)]
impl PluginLink {
    pub fn new() -> Result<PluginLink> {
        Self::new_with_wakeup(|reason| crate::tray::post_wakeup(reason as usize))
    }

    /// Variant for plugins with their own message loop. The reader uses the
    /// supplied wake-up instead of the kit tray's hidden window.
    pub fn new_with_wakeup<F>(wake: F) -> Result<PluginLink>
    where
        F: Fn(u32) + Send + 'static,
    {
        use std::io::{BufRead, BufReader};
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(std::io::stdin());
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => match HostCommand::from_line(line.trim_end()) {
                        Ok(command) => {
                            if tx.send(command).is_err() {
                                break;
                            }
                            wake(4);
                        }
                        Err(e) => eprintln!("plugin ipc: {e:#}"),
                    },
                    Err(e) => {
                        eprintln!("plugin ipc stdin: {e:#}");
                        break;
                    }
                }
            }
        });
        Ok(PluginLink {
            out: std::sync::Arc::new(std::sync::Mutex::new(std::io::stdout())),
            rx,
        })
    }

    pub fn try_recv(&self) -> Option<HostCommand> {
        self.rx.try_recv().ok()
    }

    pub fn notifier(&self) -> PluginNotifier {
        PluginNotifier {
            out: self.out.clone(),
        }
    }

    pub fn notify(&self, title: impl Into<String>, body: impl Into<String>) -> Result<()> {
        self.notifier().notify(title, body)
    }

    pub fn send(&self, event: &PluginEvent) -> Result<()> {
        send(&self.out, event)
    }

    pub fn ready(&self, menu: MenuModel, status: Option<String>) -> Result<()> {
        self.send(&PluginEvent::Ready {
            v: PROTOCOL_VERSION,
            menu,
            status,
        })
    }

    pub fn menu(&self, menu: &MenuModel) -> Result<()> {
        self.send(&PluginEvent::Menu {
            v: PROTOCOL_VERSION,
            items: menu.clone(),
        })
    }

    pub fn status(&self, text: &str) -> Result<()> {
        self.send(&PluginEvent::Status {
            v: PROTOCOL_VERSION,
            text: text.to_string(),
        })
    }
}

#[cfg(windows)]
fn send(
    out: &std::sync::Arc<std::sync::Mutex<std::io::Stdout>>,
    event: &PluginEvent,
) -> Result<()> {
    use std::io::Write;
    let line = event.to_line()?;
    let mut out = out.lock().unwrap();
    out.write_all(line.as_bytes())?;
    out.flush()?;
    Ok(())
}

fn check_version(v: u32) -> Result<()> {
    if v != PROTOCOL_VERSION {
        bail!("protocol version {v}, host supports {PROTOCOL_VERSION}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::MenuItem;

    fn model() -> MenuModel {
        MenuModel::new(vec![MenuItem::Toggle {
            id: "swap".into(),
            label: "Swap ESC/CapsLock".into(),
            checked: false,
            enabled: true,
        }])
    }

    #[test]
    fn plugin_events_round_trip() {
        let event = PluginEvent::Ready {
            v: PROTOCOL_VERSION,
            menu: model(),
            status: Some("running".into()),
        };
        let line = event.to_line().unwrap();
        assert!(line.ends_with('\n'));
        assert_eq!(PluginEvent::from_line(line.trim_end()).unwrap(), event);

        let menu = PluginEvent::Menu {
            v: PROTOCOL_VERSION,
            items: model(),
        };
        let line = menu.to_line().unwrap();
        assert!(!line.trim_end().contains('\n'));
        assert_eq!(PluginEvent::from_line(line.trim_end()).unwrap(), menu);

        let notify = PluginEvent::Notify {
            v: PROTOCOL_VERSION,
            title: "keyboard-locker".into(),
            body: "Keyboard locked".into(),
        };
        let line = notify.to_line().unwrap();
        assert_eq!(PluginEvent::from_line(line.trim_end()).unwrap(), notify);
    }

    #[test]
    fn host_commands_round_trip() {
        for command in [
            HostCommand::Invoke {
                v: PROTOCOL_VERSION,
                id: "swap".into(),
            },
            HostCommand::Reload {
                v: PROTOCOL_VERSION,
            },
            HostCommand::Quit {
                v: PROTOCOL_VERSION,
            },
        ] {
            let line = command.to_line().unwrap();
            assert_eq!(HostCommand::from_line(line.trim_end()).unwrap(), command);
        }
    }

    #[test]
    fn malformed_and_wrong_version_are_errors() {
        assert!(PluginEvent::from_line("{").is_err());
        assert!(PluginEvent::from_line(r#"{"event":"status","v":1,"text":"x"}"#).is_err());
        assert!(HostCommand::from_line(r#"{"cmd":"invoke","v":1,"id":"x"}"#).is_err());
    }
}
