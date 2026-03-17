# pnpm-catalog-lint

Lint pnpm workspaces to enforce the [`catalog:` protocol](https://pnpm.io/catalogs).

A fast, zero-config linter that ensures your monorepo consistently uses pnpm catalogs for dependency version management. Written in Rust.

## Installation

```sh
# Run directly
pnpm dlx @visiblelightio/pnpm-catalog-lint

# Or install as a dev dependency
pnpm add -D @visiblelightio/pnpm-catalog-lint
```

## Usage

Run from your workspace root:

```sh
pnpm-catalog-lint
```

Or specify a path:

```sh
pnpm-catalog-lint /path/to/workspace
```

### Example output

```
packages/app-a
  error[no-direct-version] 'react' uses "^18.2.0" in dependencies but is available in catalog: default. Use "catalog:" instead.

packages/app-b
  error[catalog-entry-exists] 'express' references "catalog:utils" in dependencies but catalog "utils" does not exist

pnpm-workspace.yaml
  warning[unused-catalog-entry] 'leftpad' ("^1.0.0") in the default catalog is never referenced

Found 3 issues (2 errors, 1 warning) in 9ms
```

## Rules

### `no-direct-version` (error)

A dependency uses a hardcoded version range (e.g. `"^18.2.0"`) but that dependency is defined in a workspace catalog. It should use `"catalog:"` instead to ensure version consistency.

Dependencies using `workspace:`, `link:`, `file:`, or `git:` protocols are skipped.

### `catalog-entry-exists` (error)

A `catalog:` or `catalog:<name>` reference points to an entry that doesn't exist in `pnpm-workspace.yaml`. This will cause `pnpm install` to fail.

Detects three cases:
- Dependency not found in the default catalog
- Named catalog doesn't exist
- Dependency not found in the specified named catalog

### `unused-catalog-entry` (warning)

A catalog entry is defined in `pnpm-workspace.yaml` but is never referenced by any `package.json` in the workspace. This may indicate a stale dependency that should be removed.

## Options

```
pnpm-catalog-lint [PATH] [OPTIONS]

Arguments:
  [PATH]  Path to the workspace root [default: .]

Options:
      --exclude-rule <RULE>         Rules to exclude (repeatable, conflicts with --only-rule)
      --only-rule <RULE>            Run only specified rules (repeatable, conflicts with --exclude-rule)
      --exclude-package <PACKAGE>    Packages to exclude (repeatable, conflicts with --only-package)
      --only-package <PACKAGE>      Run only on specified packages (repeatable, conflicts with --exclude-package)
      --exclude-dependency <DEP>    Dependencies to exclude (repeatable, conflicts with --only-dependency)
      --only-dependency <DEP>       Run only on specified dependencies (repeatable, conflicts with --exclude-dependency)
      --fail-on-warnings            Exit with non-zero code on warnings
  -h, --help                        Print help
  -V, --version                     Print version
```

### Examples

Exclude a specific rule:

```sh
pnpm-catalog-lint --exclude-rule unused-catalog-entry
```

Run only a specific rule:

```sh
pnpm-catalog-lint --only-rule no-direct-version
```

Exclude a package:

```sh
pnpm-catalog-lint --exclude-package my-legacy-app
```

Exclude a dependency:

```sh
pnpm-catalog-lint --exclude-dependency typescript
```

Fail CI on warnings too:

```sh
pnpm-catalog-lint --fail-on-warnings
```

## Development

Requires [Rust](https://www.rust-lang.org/tools/install).

```sh
# Build
cargo build

# Run tests
cargo test

# Run against a workspace
cargo run -- /path/to/workspace
```

## License

MIT
