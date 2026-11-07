pub mod config;
#[cfg(windows)]
pub mod shell;

pub const SUSPEND_LABEL: &str = "Suspend Shell Keys";

pub fn menu_model(suspended: bool) -> kit::menu::MenuModel {
    kit::menu::MenuModel::new(vec![kit::menu::MenuItem::Toggle {
        id: "suspend".into(),
        label: SUSPEND_LABEL.into(),
        checked: suspended,
        enabled: true,
    }])
}

#[cfg(test)]
mod tests {
    #[test]
    fn suspend_menu_has_a_stable_command() {
        let model = super::menu_model(true);
        assert_eq!(model.id_for_label(super::SUSPEND_LABEL), Some("suspend"));
        model.validate().unwrap();
    }
}
