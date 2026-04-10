# scopetime

*log runtime of arbitrary scope*

This crate is part of the [gitui](http://gitui.org) project and can be used to annotate arbitrary scopes to `trace` their execution times via `log`:

in your crate:
```
[dependencies]
scopetime = "0.1"
```

in your code:
```rust
fn foo(){
    scope_time!("foo");

    // ... do something u wanna measure
}
```

the resulting log looks something like this:
```
19:45:00 [TRACE] (7) scopetime: [scopetime/src/lib.rs:34] scopetime: 2 ms [my_crate::foo] @my_crate/src/bar.rs:5
```
