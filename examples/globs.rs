use std::ops::Not;

use globset::{Glob, GlobSetBuilder};

fn glob(s: &str) -> Result<Glob, ()> {
    Glob::new(s).map_err(|e| eprintln!("glob {s} failed with error:\n{e}"))
}

fn main() -> Result<(), ()> {
    let mut builder = GlobSetBuilder::new();

    builder.add(glob("**")?);
    builder.add(glob("/**")?);
    let set = builder
        .build()
        .map_err(|e| eprintln!("failed to build globs with error:\n{e}"))?;

    assert!(set.matches("hello").is_empty().not());

    Ok(())
}
