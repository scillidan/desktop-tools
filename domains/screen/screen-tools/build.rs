//! Embeds the screen-tools icon into the Windows executable.

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }
    winresource::WindowsResource::new()
        .set_icon("assets/screen-tools.ico")
        .set_icon_with_id("assets/screen-tools.suspended.ico", "2")
        .set_icon_with_id("assets/screen-tools.error.ico", "3")
        .set_icon_with_id("assets/screen-tools.disabled.ico", "4")
        .compile()
        .expect("cannot compile the icon resources");
}
