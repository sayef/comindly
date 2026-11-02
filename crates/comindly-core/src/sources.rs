//! File contents of the indexed commit, kept in the index so code can be shown without a checkout.

use std::path::Path;

use anyhow::Result;

/// Files larger than this are not stored.
const MAX_BYTES: u64 = 512 * 1024;

/// One stored file: `hash` is a content hash, so unchanged files are stored once across runs.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub repo: String,
    pub path: String,
    pub hash: String,
    pub content: String,
}

/// The text files git tracks at `root`'s HEAD; binaries, links and large files are skipped.
pub fn collect(repo: &str, root: &Path) -> Result<Vec<SourceFile>> {
    let Ok(git) = git2::Repository::open(root) else {
        return Ok(Vec::new());
    };
    let tree = git.head()?.peel_to_tree()?;
    let mut out = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |dir, e| {
        let regular = matches!(e.filemode(), 0o100644 | 0o100755);
        if let (true, Ok(name)) = (regular, e.name()) {
            let path = format!("{dir}{name}");
            if let Some(content) = read_text(&root.join(&path)) {
                out.push(SourceFile {
                    repo: repo.to_string(),
                    hash: hash(&content),
                    path,
                    content,
                });
            }
        }
        git2::TreeWalkResult::Ok
    })?;
    Ok(out)
}

fn read_text(p: &Path) -> Option<String> {
    let m = std::fs::symlink_metadata(p).ok()?;
    if !m.is_file() || m.len() > MAX_BYTES {
        return None;
    }
    let s = String::from_utf8(std::fs::read(p).ok()?).ok()?;
    (!s.contains('\0')).then_some(s)
}

fn hash(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex()[..32].to_string()
}

/// The lines around 1-based `line`: `around` before and after, numbered.
pub fn around(text: &str, line: usize, around: usize) -> Vec<(usize, String)> {
    let from = line.max(1).saturating_sub(around + 1);
    text.lines()
        .enumerate()
        .skip(from)
        .take(around * 2 + 1)
        .map(|(n, l)| (n + 1, l.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn collects_tracked_text_files_only() {
        let dir = std::env::temp_dir().join(format!("comindly-sources-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.py"), "def a():\n    return 1\n").unwrap();
        std::fs::write(dir.join("logo.png"), [0u8, 159, 146, 150]).unwrap();
        std::fs::write(dir.join("untracked.py"), "x = 1\n").unwrap();
        git(&dir, &["init", "-q"]);
        git(&dir, &["add", "src/a.py", "logo.png"]);
        git(&dir, &["commit", "-qm", "init"]);

        let files = collect("demo", &dir).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["src/a.py"]);
        assert_eq!(files[0].hash, hash("def a():\n    return 1\n"));
        assert!(
            collect("plain", &std::env::temp_dir().join("no-such-folder"))
                .unwrap()
                .is_empty()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lines_around_a_line() {
        let text = "one\ntwo\nthree\nfour\nfive\n";
        let got = around(text, 3, 1);
        assert_eq!(
            got,
            [(2, "two".into()), (3, "three".into()), (4, "four".into())]
        );
        assert_eq!(around(text, 1, 1)[0].0, 1);
    }
}
