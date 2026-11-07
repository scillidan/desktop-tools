use anyhow::{Context, Result};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use crate::config::Config;

pub const HOOK_WAKEUP_BASE: usize = 0x4b4d_0000;

pub struct Shell {
    bindings: Vec<kit::lowkey::Binding>,
    suspended: bool,
}

impl Shell {
    pub fn install(config: &Config) -> Result<Self> {
        let shell = Self {
            bindings: bindings(config)?,
            suspended: false,
        };
        shell.install_hook()?;
        Ok(shell)
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    pub fn toggle_suspended(&mut self) -> Result<()> {
        if self.suspended {
            self.install_hook()?;
            self.suspended = false;
        } else {
            kit::lowkey::uninstall();
            self.suspended = true;
        }
        Ok(())
    }

    fn install_hook(&self) -> Result<()> {
        kit::lowkey::set_bindings(&self.bindings);
        if !self.bindings.is_empty() {
            kit::lowkey::install(WM_APP, HOOK_WAKEUP_BASE)?;
        }
        Ok(())
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        kit::lowkey::uninstall();
    }
}

fn bindings(config: &Config) -> Result<Vec<kit::lowkey::Binding>> {
    config
        .suppress
        .iter()
        .enumerate()
        .map(|(id, combo)| {
            let vk = kit::hotkey::vk_of_key(combo.key)
                .with_context(|| format!("shell suppression `{combo}` has no virtual key"))?;
            Ok(kit::lowkey::Binding {
                vk: vk.0 as u8,
                ctrl: combo.ctrl,
                shift: combo.shift,
                alt: combo.alt,
                win: combo.win,
                id,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{parse, DEFAULT_CONFIG};

    #[test]
    fn every_default_binding_maps_to_a_real_virtual_key() {
        let cfg = parse(DEFAULT_CONFIG).unwrap();
        let bindings = bindings(&cfg).unwrap();
        assert_eq!(bindings.len(), 67);
        assert!(bindings.iter().all(|binding| binding.win));
        assert!(bindings.iter().any(|binding| {
            binding.vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_D.0 as u8
                && binding.ctrl
                && !binding.shift
                && !binding.alt
        }));
        assert!(
            bindings
                .iter()
                .any(|binding| binding.vk
                    == windows::Win32::UI::Input::KeyboardAndMouse::VK_E.0 as u8)
        );
    }
}
