use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/goose should have a repository root two levels up")
        .to_path_buf()
}

/// The GDK docs site offers the built-in `gdk` skill as a download. That published path is a
/// symlink to the builtin so there is only one copy in version control. If it is ever replaced
/// by a real file, the two can drift silently again.
#[test]
fn gdk_docs_download_is_symlink_to_builtin_skill() {
    let root = repo_root();
    let published = root.join("documentation/static/files/skills/gdk.md");

    let link_target = std::fs::read_link(&published).unwrap_or_else(|e| {
        panic!(
            "{} should be a symlink to the builtin skill, not a copy: {e}",
            published.display()
        )
    });

    let resolved = published
        .parent()
        .expect("published skill has a parent directory")
        .join(&link_target);
    let resolved = resolved
        .canonicalize()
        .unwrap_or_else(|e| panic!("{} does not resolve: {e}", resolved.display()));

    let builtin = root
        .join("crates/goose/src/skills/builtins/gdk.md")
        .canonicalize()
        .expect("builtin gdk skill should exist");

    assert_eq!(
        resolved,
        builtin,
        "{} points at {}, expected the builtin skill at {}",
        published.display(),
        resolved.display(),
        builtin.display()
    );

    assert!(
        link_target.is_relative(),
        "symlink target {} must be relative so checkouts and CI builds resolve it",
        link_target.display()
    );
}

/// Reading through the symlink must yield the real skill, which is what the docs site
/// serves and what `include_dir!` compiles into the binary.
#[test]
fn gdk_docs_download_reads_as_the_skill() {
    let published = repo_root().join("documentation/static/files/skills/gdk.md");
    let content = std::fs::read_to_string(&published)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", published.display()));

    assert!(
        content.starts_with("---\nname: gdk\n"),
        "{} should read as the gdk skill with its frontmatter intact",
        published.display()
    );
}

/// Guard the download link the docs page advertises, since the symlink only helps if the
/// published path is the one the docs actually point at.
#[test]
fn gdk_docs_page_links_published_skill() {
    let docs_page = repo_root().join("documentation/docs/gdk/index.md");
    let content = std::fs::read_to_string(&docs_page)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", docs_page.display()));

    assert!(
        content.contains("/files/skills/gdk.md"),
        "{} no longer links /files/skills/gdk.md",
        docs_page.display()
    );
}
