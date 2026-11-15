//! Window-owned Win-combo bindings use the shared exact-combo hook.

use crate::config::{Action, Binding};
use crate::win32::vk_of_key;
use anyhow::Result;

pub const WM_KBDHOOK: u32 = kit::lowkey::WM_KBDHOOK;

fn allow_repeat(action: Action) -> bool {
    !matches!(
        action,
        Action::ToggleWezTerm | Action::ToggleDoubleCommander | Action::SwapWindows
    )
}

pub fn set_bindings(bindings: &[Binding]) {
    let bindings: Vec<_> = bindings
        .iter()
        .enumerate()
        .filter(|(_, b)| b.combo.win)
        .filter_map(|(id, b)| {
            vk_of_key(b.combo.key).map(|vk| kit::lowkey::Binding {
                vk: vk.0 as u8,
                ctrl: b.combo.ctrl,
                shift: b.combo.shift,
                alt: b.combo.alt,
                win: b.combo.win,
                allow_repeat: allow_repeat(b.action),
                id,
            })
        })
        .collect();
    kit::lowkey::set_bindings(&bindings);
}

pub fn install() -> Result<()> {
    kit::lowkey::install(WM_KBDHOOK, 0)
}

pub fn uninstall() {
    kit::lowkey::uninstall();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_does_not_auto_repeat_but_rectangle_still_does() {
        assert!(!allow_repeat(Action::SwapWindows));
        assert!(allow_repeat(Action::MoveToLeft));
    }
}
