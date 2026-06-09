# snb_plugin_manager

Runtime plugin management commands for Shinobu.

## Commands

```text
/id
/plugin list
/plugin info <name>
/plugin load <name|dir|library-path>
/plugin unload <name>
/plugin reload <name> [name|dir|library-path]
/plugin install <github-url> [plugin-dir]
/plugin update <name|dir> [loaded-plugin-name]
```

Aliases: `/plugins`, `/pm`.

`list` shows both loaded runtime plugins and local plugin source directories
under `plugins/`, including plugins that have not been loaded yet. Names are
matched by runtime plugin name, package name, directory name, generated
`snb.toml` package name, and short aliases such as `tg` for `snb_adapter_tg`.

The plugin refuses to unload, reload, or update itself from its own command
handler.

`install` accepts GitHub repository URLs such as
`https://github.com/owner/repo.git` or `git@github.com:owner/repo.git`. It
clones the repository into `plugins/<repo>` by default, builds it with
`cargo build --release --lib`, then loads the produced release shared library.
Pass `[plugin-dir]` to choose a different local directory name.

`update` accepts a runtime name, package name, short name, directory name, or
`plugins/<dir>` path. It runs `git pull --ff-only` when the plugin directory is
its own git repository, builds through the root `cargo xtask build-plugin ...
--release` path, then loads or reloads the produced shared library. Pass
`[loaded-plugin-name]` only when automatic matching cannot tell which loaded
runtime plugin belongs to that local source directory.

## Authorization

Management commands require `event.message.is_admin == true`.
Non-admin `/plugin ...` commands are ignored without a reply.

Run `/id` to inspect the current event identity and admin flag. `/id` is public
and can reply to ordinary users.

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
