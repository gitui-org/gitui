# Key Config

The default keys are based on arrow keys to navigate.

However popular demand lead to fully customizability of the key bindings.

Create a `key_bindings.ron` file like this:
```
(
    move_left: Some(( code: Char('h'), modifiers: "")),
    move_right: Some(( code: Char('l'), modifiers: "")),
    move_up: Some(( code: Char('k'), modifiers: "")),
    move_down: Some(( code: Char('j'), modifiers: "")),

    stash_open: Some(( code: Char('l'), modifiers: "")),
    open_help: Some(( code: F(1), modifiers: "")),

    status_reset_item: Some(( code: Char('U'), modifiers: "SHIFT")),
)
```

The keybinding config file uses the [Ron file format](https://github.com/ron-rs/ron).
The location of the file depends on your OS:
`gitui` will look for an existing `/gitui` in the following order:
* `$XDG_CONFIG_HOME/gitui/` (with `XDG_CONFIG_HOME` set)
* `$HOME/.config/gitui/`
* Default OS Location:
    * `$HOME/Library/Application Support/` (mac)
    * `$HOME/.config/gitui/` (linux)
    * `%APPDATA%/gitui/` (Windows)

Key bindings are configured in `key_bindings.ron` within your first found `gitui` config folder.

See all possible keys to overwrite in gitui: [here](https://github.com/gitui-org/gitui/blob/master/src/keys/key_list.rs#L83)

Possible values for:
* `code` are defined by the type `KeyCode` in crossterm: [here](https://docs.rs/crossterm/latest/crossterm/event/enum.KeyCode.html)
* `modifiers` are defined by the type `KeyModifiers` in crossterm: [here](https://docs.rs/crossterm/latest/crossterm/event/struct.KeyModifiers.html)

Here is a [vim style key config](vim_style_key_config.ron) with `h`, `j`, `k`, `l` to navigate. Use it to copy the content into `key_bindings.ron` to get vim style key bindings.

# Key Symbols

Similar to the above GitUI allows you to change the way the UI visualizes key combos containing special keys like `enter`(default: `⏎`) and `shift`(default: `⇧`).

If we can find a file `key_symbols.ron` in the above folders we apply the overwrites in it.

Example content of this file looks like:

```
(
    enter: Some("enter"),
    shift: Some("shift-")
)
```
This example will only overwrite two symbols. Find all possible symbols to overwrite in `symbols.rs` in the type `KeySymbolsFile` ([src/keys/symbols.rs](https://github.com/gitui-org/gitui/blob/master/src/keys/symbols.rs))
