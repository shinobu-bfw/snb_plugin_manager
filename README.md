# snb_plugin_manager

Runtime plugin management commands for Shinobu.

## Commands

```text
/plugin whoami
/plugin list
/plugin info <name>
/plugin load <library-path>
/plugin unload <name>
/plugin reload <name> <library-path>
/plugin install <github-url> [plugin-dir]
/plugin update <plugin-dir> [loaded-plugin-name]
```

Aliases: `/plugins`, `/pm`.

The plugin refuses to unload or reload itself from its own command handler.
`update` is also refused when it targets `plugin_manager` itself.

`install` accepts GitHub repository URLs such as
`https://github.com/owner/repo.git` or `git@github.com:owner/repo.git`. It
clones the repository into `plugins/<repo>` by default, builds it with
`cargo build --release --lib`, then loads the produced release shared library.
Pass `[plugin-dir]` to choose a different local directory name.

`update` expects `<plugin-dir>` to be a single directory name under the runtime
`plugins/` directory. It runs `git pull --ff-only`, builds the plugin with
`cargo build --release --lib`, then reloads the produced release shared library.
Pass `[loaded-plugin-name]` when the runtime plugin name differs from the
directory name.

## Authorization

Management commands are denied by default. On first use the plugin creates:

```text
configs/plugin_manager/config.toml
```

Run `/plugin whoami` from your adapter, then add one of the exact values to the
config:

```toml
[auth]
user_ids = ["your-user-id"]
senders = []
sources = []
chat_ids = []
```

`user_ids` matches `message.from`, `senders` matches `event.sender`, `sources`
matches `event.source`, and `chat_ids` matches `message.to`.

## Build

This repository is a standalone Cargo workspace. From this directory:

```sh
cargo build --release
```

Copy the produced shared library from `target/release/` into a directory that
Shinobu scans for plugins, or load it at runtime:

```text
/plugin load path/to/snb_plugin_manager.dll
```

On Linux and macOS the library name usually starts with `lib` and ends with
`.so` or `.dylib`; on Windows it ends with `.dll`.
