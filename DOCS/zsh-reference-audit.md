---
tags: [winuxsh, zsh, reference, compatibility, plugins]
created: 2026-07-17
status: active
---

# Zsh Reference Audit for Winuxsh

> Purpose: use zsh and the zsh plugin ecosystem as the primary UX reference for
> winuxsh, while keeping shell execution semantics in rubash and Windows-native
> process behavior in winuxsh.

## Reference Snapshots

Reference source is kept outside the repository under `%TEMP%/winuxsh-reference`.

- zsh: `%TEMP%/winuxsh-reference/zsh`, commit `62103851e`
- Nushell: `%TEMP%/winuxsh-reference/nushell`, commit `eef1ddd`
- Oh My Zsh: `%TEMP%/winuxsh-reference/ohmyzsh`, commit `677a4592`
- zsh-autosuggestions: `%TEMP%/winuxsh-reference/zsh-autosuggestions`, commit `85919cd`
- zsh-syntax-highlighting: `%TEMP%/winuxsh-reference/zsh-syntax-highlighting`, commit `1d85c69`

None of these reference repos are vendored into winuxsh or added as runtime
dependencies.

## Zsh Startup Model

zsh's documented startup files are:

- `.zshenv`: sourced for all invocations; should not produce output or assume a TTY.
- `.zprofile`: login shell setup before `.zshrc`.
- `.zshrc`: interactive shell setup; aliases, options, prompt, completion, keybindings.
- `.zlogin`: login shell setup after `.zshrc`.
- `.zlogout`: login shell teardown.

For winuxsh:

- Non-interactive agent mode should not automatically load noisy interactive zsh config.
- Interactive mode can support an opt-in zsh-compatible profile importer.
- `ZDOTDIR` can be respected for locating zsh-style config, defaulting to Windows home.
- `~` continues to mean Windows user home, not an MSYS2/Git Bash root.

## Zsh UX Features Worth Targeting

### Startup/Profile Compatibility

- `ZDOTDIR`
- `.zshrc` discovery
- simple `export`, `alias`, `PATH/path`, `fpath`, `ZSH_THEME`, and `plugins=(...)`
- Oh My Zsh-style `ZSH`, `ZSH_CUSTOM`, `ZSH_CACHE_DIR`
- selected `zstyle` records for completion and plugin config

### Completion

- `fpath`-based completion discovery.
- `_cmd` files with `#compdef`.
- `autoload -Uz compinit` / `compinit` as a signal to enable completion.
- `compdef _foo foo` mappings.
- `zstyle ':completion:*'` settings:
  - matcher-list
  - ignored-patterns
  - verbose/group/format settings
  - cache-path/use-cache

Implementation should translate a useful subset into winuxsh TOML completion
definitions and completion settings. It should not run the zsh completion system.

### Line Editor / ZLE

zsh plugins commonly depend on:

- `bindkey`
- `zle -N`
- `BUFFER`, `CURSOR`, `POSTDISPLAY`
- `region_highlight`
- `precmd`, `preexec`, `chpwd`, `zle-line-init`, `zle-line-finish`
- `add-zsh-hook`
- sometimes `zpty` for async completion capture

For winuxsh this maps to native reedline features, not direct execution of ZLE
scripts. The compatibility goal is to honor familiar config variable names and
plugin intent where possible.

### Prompt/Themes

zsh and Oh My Zsh themes use:

- `PROMPT`
- `RPROMPT`
- prompt escapes like `%~`, `%n`, `%m`, `%#`
- color escapes such as `%F{red}`, `%B`, `%b`, `%f`
- theme variables such as `ZSH_THEME_GIT_PROMPT_PREFIX`
- helper functions like `git_prompt_info`
- `precmd` hooks for dynamic prompt data

Winuxsh should translate common prompt escapes and provide native equivalents
for common Git prompt segments rather than running arbitrary theme scripts.

### Oh My Zsh Plugin Shape

Oh My Zsh loads:

- `plugins=(git npm ... )` from `.zshrc`
- `$ZSH/oh-my-zsh.sh`
- `$ZSH/lib/*.zsh`
- `$ZSH/plugins/<name>/<name>.plugin.zsh`
- `$ZSH/plugins/<name>/_<name>` completion files
- `$ZSH_CUSTOM/*.zsh`
- `$ZSH_CUSTOM/plugins/<name>/...`
- `$ZSH/themes/<name>.zsh-theme`

Winuxsh can emulate this layout for discovery/import, but should avoid blindly
sourcing zsh scripts.

## Plugin Compatibility Tiers

### Tier 1 - Directly Useful

- Completion-only plugins with `_cmd` files and `#compdef`.
- Plugins that only define aliases for external commands.
- Simple environment/profile snippets.

These should be imported or translated first.

### Tier 2 - Translatable

- Oh My Zsh themes using common `PROMPT`/`RPROMPT` escapes.
- Plugins that define simple functions compatible with rubash/bash syntax.
- `zstyle` settings that map to winuxsh completion/menu behavior.

These need a translator/importer and clear warnings for unsupported lines.

### Tier 3 - Native Reimplementation

- zsh-autosuggestions.
- zsh-syntax-highlighting.
- history substring search.
- advanced completion menu behavior.

These should be implemented in Rust/reedline while honoring familiar config
variables where possible, such as `ZSH_AUTOSUGGEST_*` and
`ZSH_HIGHLIGHT_STYLES`.

### Tier 4 - Unsupported or Deferred

- Arbitrary `zle -N` widget wrapping.
- `zmodload`.
- `zpty`.
- deep zsh array/parameter expansion used inside plugin internals.
- plugins that depend on a real zsh interpreter.

Unsupported features should fail softly during import with diagnostics, not
break shell startup.

## Strategic Conclusion

Winuxsh should be **zsh-compatible at the profile/plugin intent layer**, not a
zsh interpreter. The right path is:

1. Keep rubash as the shell semantic engine.
2. Add a zsh profile/plugin importer for common config and completion assets.
3. Implement zsh-like interactive features natively in reedline.
4. Provide Oh-My-Winuxsh as a compatibility/package layer for themes,
   completions, aliases, and native UX modules.
