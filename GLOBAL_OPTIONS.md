# Global options

You can set global options that will take effect over the entire system for GitUI. Note however, that each time GitUI closes, it saves a local config file (`./.git/gitui.ron`) that has a higher order precedence. Meaning, if you had changed global config while already having local file for given repository, the changes will not be visible.

The reason behind this decision is local file has additional fields saved which facilitates GitUI for a specific repository (eg. `tab` which allows to open up GitUI in the last tab it was closed with).

The precedence of fetching the options is:

1. Use **local** options file. _If not found then:_
2. Use **global** options file. _If not found then:_
3. Use default values.

To set up global options create `gitui.ron` file:

```
(
    diff: (
        ignore_whitespace: false,
        context: 3,
        interhunk_lines: 2,
    ),
    status_show_untracked: None,
)
```

The options file format based on the [Ron file format](https://github.com/ron-rs/ron).
The location of the file depends on your OS:

- `$HOME/.config/gitui/gitui.ron` (mac)
- `$XDG_CONFIG_HOME/gitui/gitui.ron` (linux using XDG)
- `$HOME/.config/gitui/gitui.ron` (linux)
- `%APPDATA%/gitui/gitui.ron` (Windows)
