# Proposal: Improved `jgl fetch` Summary Display

## Current state

```
  changed    myrepo (rebased)
  unchanged  other
  timed out  slow-repo
  error      failed-repo: permission denied
```

Flat list, unsorted, no visual grouping. Rebase info is parenthetical.

## Proposed output

```
 fetch  rebase  repo             detail
 -----  ------  ----             ------
  E      ·      broken-repo      permission denied
  T      ·      repo3
  +      !      work/myrepo
  +      U      private/myrepo
  +      +      myrepo
  +      ·      infra
  ·      ·      libs

fetch: E=error  T=timed out  +=changed  ·=unchanged
rebase: !=conflicts kept  U=undone due to conflicts  +=rebased  ·=unchanged

7 repos: 1 fetch error, 1 timed out, 4 fetch changed, 1 unchanged, 1 rebased, 1 rebase conflicts, 1 rebase undone
```

## Design decisions

### Two columns, not one

Fetch and rebase are independent operations with independent outcomes. A fetch can fail while a rebase succeeds (on previously fetched data), or vice versa. Flattening them into a single combined status loses information. Two columns cost ~4 characters of width and eliminate ambiguity.

### Rebase column hidden when not applicable

When `--rebase` is not passed, the rebase column and its legend are omitted entirely.

### Blank rebase cell vs `·`

A blank rebase cell means rebase was **not attempted** because the repo opts out. `·` means rebase **ran** but nothing moved (trunk hadn't advanced). These are semantically different: blank = not applicable, `·` = ran, no-op.

> **Note:** The current code does not distinguish "rebased, nothing moved" from "rebased successfully." Implementing `·` requires detecting a no-op rebase by parsing `jj rebase` output.

### Header always printed

The header row (`fetch  rebase  repo  detail` with dashes) is always printed, even for a single repo. Keeps output consistent and scannable.

### All output to stdout

The entire table (including errors/timeouts) is written to stdout as a single unified table. The current split between stdout and stderr is dropped.

### Sorted by worst status across either column

Repos are sorted by the worst status across both fetch and rebase columns. Alphabetical within each group. With many repos the common case is "everything is fine" — grouping surfaces the 1-2 problems instantly.

Priority order — fetch: `E` > `T` > `+` > `·`. Rebase: `E` > `!` > `U` > `+` > `·` > blank.

### Legend only shows codes that appeared

e.g. if no repo errored, `E=error` is omitted from the legend.

## Fetch status symbols

| Char | Meaning | Color |
|------|---------|-------|
| `E`  | error   | red   |
| `T`  | timeout | red   |
| `+`  | changed | green |
| `·`  | unchanged | dim/grey |

## Rebase status symbols

| Char | Meaning | Color |
|------|---------|-------|
| `E`  | error (non-conflict failure) | red |
| `!`  | conflicts kept | yellow |
| `U`  | undone (had conflicts) | yellow |
| `+`  | rebased | green |
| `·`  | no change (rebase ran, nothing moved) | dim/grey |
| ` `  | not attempted (repo opts out of rebase) | blank |

## Notable combinations

- **`Unchanged` + `Rebased`**: possible when a previous fetch brought in changes that weren't rebased at the time, or when the user is working on a change not directly based on `trunk()`.
- **`Failed`/`TimedOut` + `Rebased`**: Rebase runs regardless of fetch outcome — local changes may still benefit from rebasing on previously fetched data.

## Format

```
{fetch_char}  [{rebase_char}]  {label}[  {detail}]
```

- **fetch_char**: single colored character, always present
- **rebase_char**: single colored character, omitted when `--rebase` is not active
- **label**: right-padded to align the detail column
- **detail**: only shown for error messages. Omitted for rebase issues (conflicts can be inspected via `jj`).

## Summary line

Always printed last. Only lists categories with non-zero counts (same rule as the legend):

```
7 repos: 1 fetch error, 1 timed out, 4 fetch changed, 1 unchanged, 1 rebased, 1 rebase conflicts, 1 rebase undone
```

## Color handling

Colors (red, green, yellow, dim/grey) are applied when stdout is a TTY. Colors are disabled when stdout is not a TTY or when the `NO_COLOR` environment variable is set.

## Future extensibility

### Verbose mode (`-v`)

Print the table, legend etc. the same way, then print details per repository that changed / errored in any way. It won't fit in a table.

### JSON output (`--format=json`)

`FetchResult` already captures all the data. Add `serde::Serialize` to `FetchResult`, `FetchStatus`, `RebaseStatus` and emit JSON. The human display and JSON output are independent renderers over the same `Vec<FetchResult>` — no structural changes needed.

### Filtering (`--only=errors`)

Since results are already categorized, filtering is trivial post-hoc over the results vec.
