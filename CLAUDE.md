# gitui — working notes for Claude

gitui is a Rust TUI git client. Cargo **workspace**: root crate `gitui` (the `src/` TUI binary) +
members `asyncgit` (git logic over `git2`/`gix`), `filetreelist`, `git2-hooks`, `git2-testing`,
`scopetime`. MSRV **1.88** (`Cargo.toml` `rust-version`, `.clippy.toml` `msrv`, CI matrix — keep
all three in sync if ever bumped).

## Feedback loop (run these — the reviewer here does not read Rust, so tooling IS the review)

- **Fast inner loop while editing:** `cargo check -p asyncgit` (backend) / `cargo check` (whole
  workspace). Never iterate with `--release` — LTO + `opt-level="z"` makes it very slow.
- **Format before every check:** `cargo fmt`. Formatting is unusual (`rustfmt.toml`:
  `max_width = 70`, `hard_tabs = true`) — hand-written code almost never matches, and
  `cargo fmt -- --check` is a CI gate.
- **Full gate before declaring done / committing:** `make check`
  = `fmt` (`cargo fmt -- --check`) + `clippy` (`cargo clippy --workspace --all-features`) +
  `test` (`cargo nextest run --workspace`) + `sort` (`tombi format --check`) + `deny`
  (`cargo deny check`). CI calls the same `make` targets, so green locally ≈ green CI.
- **Subset fallback** if a tool is missing: `cargo fmt -- --check` +
  `cargo clippy --workspace --all-features` + `cargo test --workspace` (plain `cargo test` works;
  it just also runs doctests that nextest skips).
- **Run a single crate's tests:** `cargo test -p asyncgit <name_filter>`.

Required tooling: `cargo-nextest`, `cargo-deny`, `tombi` (installed via mise), plus `python`,
`gpgsm`/`gpg`, `openssl`, `perl`, a C compiler (vendored OpenSSL + bundled libgit2 build from
source on first compile — the first build is slow, later ones are cached).

## Hard rules clippy enforces (these fail the build, not just warn)

Crate roots carry `#![deny(clippy::all, perf, nursery, pedantic, cargo)]` plus bans below.
- **No `unwrap` / `expect` / `panic!` in production code** — use `?`, `ok_or(...)`,
  `unwrap_or_default()`, `map_or(...)`. (`unwrap`/`expect` are fine in `#[cfg(test)]` code.)
- `asyncgit` has `#![forbid(missing_docs)]` — **every `pub` item needs a `///` doc comment**.
- `#![forbid(unsafe_code)]` in the binary crate.

## Other gotchas

- **UI output is snapshot-tested with `insta`** (`src/snapshots/*.snap`). If you change rendered
  output, tests fail with a diff — review and accept via `cargo insta review` / `cargo insta
  accept`, then commit the updated `.snap` files. (Install once: `cargo install cargo-insta`.)
- **CHANGELOG.md** must gain an entry under `## Unreleased` (`Added`/`Changed`/`Fixes`) or the CI
  `log-test` job fails. Format: Keep-a-Changelog, with issue/PR links.
- Only the `stable` and `1.88` CI rows gate (nightly is `continue-on-error`). `cargo +nightly
  udeps` runs in CI but not in `make check`, so **don't add unused dependencies**.
- Production code never shells out to the `git` binary; use `git2`/`gix`. The only
  `Command::new("git")` calls live in test helpers (`debug_cmd_print` in `asyncgit/src/sync/mod.rs`).

## Where things live (patterns to copy)

- Sync git ops: `asyncgit/src/sync/<feature>.rs`, re-exported from `asyncgit/src/sync/mod.rs`. Each
  op: `pub fn f(repo_path: &RepoPath, ...) -> Result<T> { scope_time!("f"); let repo =
  repo(repo_path)?; ... }`. Test helpers (`repo_init`, `debug_cmd_print`) in `sync/mod.rs::tests`.
- Popups (Branches/Submodules/etc.): `src/popups/<name>.rs`, registered in `src/popups/mod.rs` and
  the `setup_popups!` macro in `src/app.rs`; opened via an `InternalEvent` in `src/queue.rs`.
- Components implement `Component` + `DrawableComponent` (`src/components/mod.rs`). Non-commit list
  views use `VerticalScroll` + `ui::scrolllist::draw_list` (see `src/popups/submodules.rs`).
- Keybindings: `src/keys/key_list.rs` (`KeysList` struct + `Default`). Command/help strings:
  `src/strings.rs`.
