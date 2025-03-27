# Themes

default on light terminal:
![](assets/light-theme.png)

## Configuration

To change the colors of the default theme you need to add a `theme.ron` file that contains the colors you want to override. Note that you don’t have to specify the full theme anymore (as of 0.23). Instead, it is sufficient to override just the values that you want to differ from their default values.

The file uses the [Ron format](https://github.com/ron-rs/ron) and is located at one of the following paths, depending on your operating system:

* `$HOME/.config/gitui/theme.ron` (mac)
* `$XDG_CONFIG_HOME/gitui/theme.ron` (linux using XDG)
* `$HOME/.config/gitui/theme.ron` (linux)
* `%APPDATA%/gitui/theme.ron` (Windows)

Alternatively, you can create a theme in the same directory mentioned above and use it with the `-t` flag followed by the name of the file in the directory. E.g. If you are on linux calling `gitui -t arc.ron`, this will load the theme in `$XDG_CONFIG_HOME/gitui/arc.ron` or `$HOME/.config/gitui/arc.ron`.

Example theme override:

```ron
(
    selection_bg: Some("Blue"),
    selection_fg: Some("#ffffff"),
)
```

Note that you need to wrap values in `Some` due to the way the overrides work (as of 0.23).

Notes:

* rgb colors might not be supported in every terminal.
* using a color like `yellow` might appear in whatever your terminal/theme defines for `yellow`
* valid colors can be found in ratatui's [Color](https://docs.rs/ratatui/latest/ratatui/style/enum.Color.html) struct.
* all customizable theme elements can be found in [`style.rs` in the `impl Default for Theme` block](https://github.com/gitui-org/gitui/blob/master/src/ui/style.rs#L305)

## Preset Themes

You can find preset themes by Catppuccin [here](https://github.com/catppuccin/gitui.git).

## Syntax Highlighting

The syntax highlighting theme can be defined using the element `syntax`. Both [default themes of the syntect library](https://github.com/trishume/syntect/blob/7fe13c0fd53cdfa0f9fea1aa14c5ba37f81d8b71/src/dumps.rs#L215) and custom themes are supported.

Example syntax theme:
```ron
(
    syntax: Some("InspiredGitHub"),
)
```

Custom themes are located in the [configuration directory](#configuration), are using TextMate's theme format and must have a `.tmTheme` file extension. To load a custom theme, `syntax` must be set to the file name without the file extension. For example, to load [`Blackboard.tmTheme`](https://raw.githubusercontent.com/filmgirl/TextMate-Themes/refs/heads/master/Blackboard.tmTheme), place the file next to `theme.ron` and set:
```ron
(
    syntax: Some("Blackboard"),
)
```

[filmgirl/TextMate-Themes](https://github.com/filmgirl/TextMate-Themes) offers many [beautiful](https://inkdeep.github.io/TextMate-Themes) TextMate themes to choose from.

## Customizing line breaks

If you want to change how the line break is displayed in the diff, you can also specify `line_break` in your `theme.ron`:

```ron
(
    line_break: Some("¶"),
)
```

Note that if you want to turn it off, you should use a blank string:

```ron
(
    line_break: Some(""),
)
```
## Customizing selection

By default the `selection_fg` color is used to color the text of the selected line.
Diff line, filename, commit hashes, time and author are re-colored with `selection_fg` color.
This can be changed by specifying the `use_selection_fg` boolean in your `theme.ron`:

```
(
    use_selection_fg: Some(false),
)
```

By default, `use_selection_fg` is set to `true`.
