use crate::Combo;
use anyhow::{Context, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding<A> {
    pub combo: Combo,
    pub action: A,
}

pub struct ConfigPart<A, S> {
    pub bindings: Vec<Binding<A>>,
    pub settings: S,
}

/// Strict module-owned config: every row is either a recognized setting or a
/// binding. Legacy `window-tools.toml` keeps its older tolerant parser.
pub fn parse<A, S, F, G>(
    text: &str,
    mut settings: S,
    mut action: F,
    mut setting: G,
) -> Result<ConfigPart<A, S>>
where
    F: FnMut(&str) -> Option<A>,
    G: FnMut(&mut S, &str, &toml::Value) -> Result<bool>,
{
    let value: toml::Value = text.parse().context("module config is not valid TOML")?;
    let table = value
        .as_table()
        .context("module config root must be a TOML table")?;
    let mut bindings = Vec::new();
    for (combo, value) in table {
        if setting(&mut settings, combo, value)? {
            continue;
        }
        let name = value
            .as_str()
            .with_context(|| format!("`{combo}` action must be a string"))?;
        let action =
            action(name).with_context(|| format!("unknown action `{name}` for `{combo}`"))?;
        let combo = combo
            .parse::<Combo>()
            .with_context(|| format!("invalid hotkey `{combo}`"))?;
        bindings.push(Binding { combo, action });
    }
    bindings.sort_by_key(|b| b.combo);
    Ok(ConfigPart { bindings, settings })
}
