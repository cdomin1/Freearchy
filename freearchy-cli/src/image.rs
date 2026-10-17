use anyhow::Result;

use crate::util;

/// Thin wrapper — the heavy lifting lives in image/build-image.sh so it can
/// also be run standalone on any FreeBSD box.
pub fn build(release: &str) -> Result<()> {
    util::ensure_freebsd()?;
    let script = util::repo_root()?.join("image/build-image.sh");
    util::run("sh", &[script.to_str().unwrap(), release])
}
