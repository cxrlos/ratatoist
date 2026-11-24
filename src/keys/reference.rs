use std::fmt::Write;

use super::{ACTIONS, Context, Key, Preset};

/// The keymap reference, `docs/keymap.md`: every action of the registry with
/// its config id, its keys in each preset in config notation and its
/// description, one table per context. An action bound in several contexts
/// is listed in each.
pub fn keymap_reference() -> String {
    let mut out = String::from(
        "# Keymap\n\
         \n\
         <!-- Generated from the action registry in src/keys/registry.rs. \
         Do not edit by hand: run `UPDATE_KEYMAP=1 cargo test --test keymap_reference`. -->\n\
         \n\
         These are ratatoist's default keys, in the `vim` preset and in the `standard` \
         preset that `[keys] preset = \"standard\"` picks. Keys are written in the vim \
         notation `config.toml` uses, and each table heading names the config table that \
         rebinds its actions; see [Configuration](config.md#keys).\n\
         \n\
         The projects pane takes the Projects keys, then the Projects and Labels keys. \
         Everywhere keys apply under all of them, and an overlay takes only the Overlays keys.\n\
         \n\
         While the add-task form or a prompt has a text field, a character typed without \
         Ctrl or Alt goes into the field instead, and so do `<Backspace>`, `<Delete>`, `<Left>`, \
         `<Right>`, `<Home>`, `<End>`, `<C-w>` (delete the word before the cursor) and \
         pasted text. Esc still closes it without sending anything, and Enter submits.\n",
    );
    for context in Context::ALL {
        let _ = write!(
            out,
            "\n## {} (`[keys.{}]`)\n\n\
             | Action | Keys (vim) | Keys (standard) | Description |\n\
             | --- | --- | --- | --- |\n",
            context.title(),
            context.name()
        );
        for spec in ACTIONS
            .iter()
            .filter(|spec| spec.contexts.contains(&context))
        {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} |",
                spec.id,
                keys(Preset::Vim.default_keys(spec)),
                keys(Preset::Standard.default_keys(spec)),
                spec.description
            );
        }
    }
    out
}

fn keys(sequences: &[&[Key]]) -> String {
    sequences
        .iter()
        .map(|sequence| format!("`{}`", notation(sequence)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A key sequence as `config.toml` writes it: plain characters as themselves,
/// anything else inside `<...>`.
fn notation(sequence: &[Key]) -> String {
    sequence
        .iter()
        .map(|key| match key.typed() {
            Some('<') => "<lt>".to_owned(),
            Some(' ') => "<Space>".to_owned(),
            Some(c) => c.to_string(),
            None => format!("<{key}>"),
        })
        .collect()
}
