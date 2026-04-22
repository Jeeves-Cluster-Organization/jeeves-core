//! Session tree stored as newline-delimited JSON.
//!
//! Each line is an `Entry` with an `id` and optional `parent_id`, forming
//! a tree. This powers pi-style `/tree`, `/fork`, and `/clone` operations.
//! The core only owns the primitives — what "fork" or "clone" *means* in UX
//! terms is the harness's call.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug)]
pub struct Session {
    path: PathBuf,
}

impl Session {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            std::fs::File::create(&path)?;
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, entry: &Entry) -> Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.path)?;
        let line = serde_json::to_string(entry)?;
        writeln!(f, "{line}")?;
        Ok(())
    }

    pub fn read_all(&self) -> Result<Vec<Entry>> {
        let f = std::fs::File::open(&self.path)?;
        let reader = BufReader::new(f);
        let mut out = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            out.push(serde_json::from_str(&line)?);
        }
        Ok(out)
    }

    /// Path from root to `leaf_id`, ordered root-first.
    pub fn path_to(&self, leaf_id: &str) -> Result<Vec<Entry>> {
        let all = self.read_all()?;
        let mut by_id = std::collections::HashMap::new();
        for e in &all {
            by_id.insert(e.id.clone(), e.clone());
        }
        let mut chain = Vec::new();
        let mut cursor = by_id
            .get(leaf_id)
            .cloned()
            .ok_or_else(|| Error::Invalid(format!("unknown entry id: {leaf_id}")))?;
        loop {
            let parent = cursor.parent_id.clone();
            chain.push(cursor);
            match parent {
                Some(pid) => {
                    cursor = by_id
                        .get(&pid)
                        .cloned()
                        .ok_or_else(|| Error::Invalid(format!("dangling parent: {pid}")))?
                }
                None => break,
            }
        }
        chain.reverse();
        Ok(chain)
    }

    /// Copy the entries reachable from `leaf_id` into a new session file.
    pub fn clone_to(&self, leaf_id: &str, dst: impl AsRef<Path>) -> Result<Session> {
        let chain = self.path_to(leaf_id)?;
        let dst = dst.as_ref().to_path_buf();
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::File::create(&dst)?;
        for e in &chain {
            let line = serde_json::to_string(e)?;
            writeln!(f, "{line}")?;
        }
        Ok(Session { path: dst })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_read_roundtrip() {
        let dir = tempdir().unwrap();
        let s = Session::open(dir.path().join("a.jsonl")).unwrap();
        s.append(&Entry {
            id: "1".into(),
            parent_id: None,
            payload: serde_json::json!({"n":1}),
        })
        .unwrap();
        s.append(&Entry {
            id: "2".into(),
            parent_id: Some("1".into()),
            payload: serde_json::json!({"n":2}),
        })
        .unwrap();
        let all = s.read_all().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].parent_id.as_deref(), Some("1"));
    }

    #[test]
    fn path_to_leaf_is_root_first() {
        let dir = tempdir().unwrap();
        let s = Session::open(dir.path().join("a.jsonl")).unwrap();
        s.append(&Entry {
            id: "a".into(),
            parent_id: None,
            payload: serde_json::json!({}),
        })
        .unwrap();
        s.append(&Entry {
            id: "b".into(),
            parent_id: Some("a".into()),
            payload: serde_json::json!({}),
        })
        .unwrap();
        s.append(&Entry {
            id: "c".into(),
            parent_id: Some("b".into()),
            payload: serde_json::json!({}),
        })
        .unwrap();
        let chain = s.path_to("c").unwrap();
        let ids: Vec<_> = chain.iter().map(|e| e.id.clone()).collect();
        assert_eq!(ids, vec!["a".to_string(), "b".into(), "c".into()]);
    }
}
