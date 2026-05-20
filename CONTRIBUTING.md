# Contributing

We’re glad you found this document that is intended to make contributing to
GitUI as easy as possible!

## Building GitUI

In order to build GitUI on your machine, follow the instructions in the
[“Build” section](./README.md#build).

## Getting help

There’s a [Discord server][discord-server] you can join if you get stuck or
don’t know where to start. People are happy to answer any questions you might
have!

## Getting started

If you are looking for something to work on, but don’t yet know what might be a
good first issue, you can take a look at [issues labelled with
`good-first-issue`][good-first-issues]. They have been selected to not require
too much context so that people not familiar with the codebase yet can still
make a contribution.

## Cross-compiling for OpenHarmony

GitUI can be built for OpenHarmony (target `aarch64-unknown-linux-ohos`). This requires the [OpenHarmony SDK](https://www.openharmony.cn/) native toolchain.

First, install the `aarch64-unknown-linux-ohos` Rust target:

```sh
rustup target add aarch64-unknown-linux-ohos
```

Set the following environment variables (adjust paths to your OHOS SDK location):

```sh
export OHOS_SDK=/path/to/ohos-sdk
export CC_aarch64_unknown_linux_ohos="$OHOS_SDK/native/llvm/bin/clang"
export CXX_aarch64_unknown_linux_ohos="$OHOS_SDK/native/llvm/bin/clang++"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_LINKER="$OHOS_SDK/native/llvm/bin/clang"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_AR="$OHOS_SDK/native/llvm/bin/llvm-ar"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_RUSTFLAGS="-C link-arg=--sysroot=$OHOS_SDK/native/sysroot -C link-arg=-L$OHOS_SDK/native/sysroot/usr/lib/aarch64-linux-ohos -C link-arg=-Wl,--allow-multiple-definition -C link-arg=-Wl,--undefined-version -C link-arg=-Wl,--defsym=__xpg_strerror_r=0"
```

Then build:

```sh
cargo build --target aarch64-unknown-linux-ohos --release
```

> **Note:** On OpenHarmony the user/process model is sandbox-based. GitUI automatically disables libgit2's owner validation on OHOS targets to avoid spurious "not owned by current user" errors.

[discord-server]: https://discord.gg/rZv4uxSQx3
[good-first-issues]: https://github.com/gitui-org/gitui/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22
