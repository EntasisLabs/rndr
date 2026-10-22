# rndr_std

[`std::fs`](https://doc.rust-lang.org/std/fs/) backend for the [rndr](https://github.com/EntasisLabs/rndr) file tree.

```rust
use rndr_std::StdFsPlugin;

app.add_plugin(StdFsPlugin);
```

`StdFsPlugin` inserts `StdFs` as the `Files` resource. The in-memory backend lives in `rndr_core`.

License: MIT.
