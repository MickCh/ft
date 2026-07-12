# Code Review — `ft`

Date: 2026-07-11. Scope: full codebase (all modules and tests). `cargo clippy --all-targets -- -D warnings` and `cargo test` (65 tests) both pass. Findings marked *(verified)* were reproduced against the built binary.

**Overall: high-quality code.** For a Rust learning project, the architecture and idiomatic style are above what many production CLI tools ship with. One real data-loss bug was found, plus a few design points worth acting on.

## 🔴 Issues found

### 1. `-o` pointing at the input file silently destroys data *(verified)*

```
$ printf 'one\ntwo\nthree\n' > same.txt
$ ft -R 1-2 -o same.txt same.txt   # exit 0, no error
$ wc -c same.txt                   # 0 bytes
```

In `run_streaming` (`src/main.rs:32`) the reader is opened before `File::create`, but `create` truncates the same inode, so the first read returns EOF and the file ends up empty — with exit code 0. This is exactly the scenario the whole `--in-place` machinery protects against, bypassed by a single `-o`.

**Fix:** validate in `Config::try_from` or in `main`: compare the paths (ideally after `fs::canonicalize`, to catch `./same.txt` vs `same.txt`) and reject with a clear error — `sort -o`, for instance, handles this case.

**Status: fixed** — `run_streaming` rejects an output that canonicalizes to the input path (`AppError::OutputIsInput`), covered by e2e tests including the symlink-alias case.

### 2. `--in-place` on a symlink replaces the symlink with a regular file *(verified)*

```
$ ln -s target.txt link.txt
$ ft -i --tac link.txt    # link.txt is now a regular file, target.txt untouched
```

`rename` swaps out the symlink itself, not its target. This matches `sed -i` without `--follow-symlinks`, so it is an acceptable trade-off — but it should be a conscious decision: either document it in `--help`, or resolve the path (`fs::canonicalize`) before building the temp sibling and renaming.

**Status: fixed** — `run_in_place` canonicalizes the path first, so the link's target is edited and the symlink survives; covered by an e2e test.

### 3. A broken pipe ends with an error *(verified)*

```
$ ft big.txt | head -1
Error: Processing error: Broken pipe (os error 32)   # exit 1
```

For a text-filtering tool this is an everyday use case, not an error. Unix convention: on `ErrorKind::BrokenPipe`, exit quietly (status 0 or 141). Easy fix in `main()` — one extra match on the error kind.

**Status: fixed** — `main()` treats a `BrokenPipe` processing error as a successful exit; covered by an e2e test that closes the read end of the pipe.

### 4. (Minor) Rename atomicity ≠ durability

`run_in_place` does `flush` + `rename` but no `File::sync_all` before the rename. After a system crash right after the rename, the file can be empty/incomplete despite the "atomic" swap — rename is atomic in the namespace, not for data durability. Many tools skip this deliberately (performance cost); treat as an optional hardening.

**Status: fixed** — the temp file is `sync_all`ed after `set_permissions`, before the rename.

## 🟡 Architecture — SOLID and the stairway pattern

Strong points: the module split is exemplary for SRP (parsing → `cli_args`, column semantics → `columns`, pure text manipulation → `text`, orchestration → `file_processor`), only `main.rs` touches the filesystem, and `LineTransform`/`LinePredicate` are textbook OCP — a new operation is a new type, not a new branch in the loop. Testing through injected `BufRead`/`Write` is dependency inversion in practice.

One place where the stairway breaks: **`transform`, `predicate` and `file_processor` depend on `cli_args::Config`** — the engine layer depends on the CLI-argument representation layer, a dependency pointing up the stairs. `build_pipeline(&Config)` in `src/transform.rs:189` means the engine cannot be used without a clap-derived config.

**Suggested fix:** move the `build_pipeline`/`build_predicate` factories and the `FileProcessor::new` derivation into a composition layer (e.g. `Config` produces the components, or a dedicated `compose` module), and have the engine accept ready-made `Vec<Box<dyn LineTransform>>`, `RangeSpec`, etc. The `text`, `columns` and `ranges` modules are already clean in this respect.

Second point — **`Config` does not make invalid states unrepresentable**:

- `sort`/`tac`/`shuffle` are three bools whose mutual exclusion is policed by clap, and `FileProcessor::new` re-derives the `Reorder` enum from them anyway. If `Config` held an `Option<ReorderMode>` (and analogously an `Option<SortSpec>`), the invariant would be structural and the `if`/`else if` cascade in `src/file_processor.rs:115` would disappear.
- `finds: Vec<FindPattern>` + `replace_strings: Vec<String>` are parallel vectors whose equal length is policed by validation. A `Vec<Replacement { find, replace }>` (paired up in `try_from`) would remove both the `FindReplaceCountMismatch` validation as a separate state and the `zip` in `build_pipeline`.
- `has_column_operation()` (`src/cli_args/config.rs:80`) is a list that must be kept in sync by hand with every new operation — a silent failure point when extending the tool. It would fall out for free from the restructuring above (e.g. "is the list of column operations non-empty").

## 🟢 Rust idioms

Very good: consistent `Result` propagation (zero `unwrap` outside tests; the only `expect`, in `ReplaceInColumnsIgnoreCase::new`, is justified and commented), errors with hand-written `Display` plus a correct `source()`, `Cow` in `apply_transforms` for the allocation-free path, `NumericKey` using `total_cmp` instead of the `partial_cmp().unwrap()` trap, `sort_by_cached_key` for stability and performance, `saturating_add/sub` in range arithmetic, validation inside clap value parsers rather than after the fact. UTF-8 handling (`char`-based indexing with a single char→byte mapping pass in `text::byte_range`, no per-line `Vec<char>`) is done properly and densely tested.

Minor points to consider:

- `LineTransform::apply` returns `String`, so every transform allocates even when it changed nothing (e.g. a replace with no match). `fn apply<'a>(&self, line: &'a str) -> Cow<'a, str>` would let transforms return a borrow — consistent with what `apply_transforms` already does at the edges.
- `use std::io::prelude::*` in `file_processor.rs` next to explicit imports — explicit `BufRead`/`Write` would suffice.
- The name `FileProcessor` is misleading in that the type deliberately does *not* touch files (that is its virtue) — `StreamProcessor`/`Processor` would express the intent.
- `parse_bound` accepts `+5` (the `usize` parser tolerates a leading plus) — cosmetic.
- Hand-written error enums are good for learning; in "grown-up" code `thiserror` would remove ~80 lines of boilerplate from `error.rs`/`config_error.rs`.

## 🟢 Remaining safety

Beyond items 1–2 above, this is solid: `create_new` on the predictable temp name protects against a planted file/symlink, permissions are preserved, temp cleanup runs on every error path, and same-directory rename guarantees atomicity. User-supplied regexes go through the `regex` crate, which guarantees linear-time matching — no ReDoS risk. UTF-8 validation reports the line number, and untouched lines pass through byte-for-byte without forcing UTF-8 on the whole file — well thought out.

## Summary

Fix first: the **`-o` == input file guard** (the only real data-loss bug), then quiet exit on `BrokenPipe`. Architecturally, the biggest win is inverting the engine's dependency on `Config` and replacing the boolean flags / parallel vectors with structures that encode their invariants — which happen to be exactly the exercises that teach the most in idiomatic Rust. The rest is cosmetic; the foundation (streaming with buffering only where the semantics demand it, pure text helpers, exemplary tests) is very good.
