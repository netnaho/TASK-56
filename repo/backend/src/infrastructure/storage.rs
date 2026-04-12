//! Local attachment storage — filesystem-backed blob storage.
//!
//! Design:
//!
//! * The storage root comes from `AppConfig::attachment_storage_path` and is
//!   expected to be a Docker volume (see `docker-compose.yml`).
//! * Filenames on disk are **always UUIDs** — client-supplied names are
//!   never part of a path. The original filename lives only in the
//!   `attachments.original_filename` column for display purposes.
//! * Files are bucketed under `{root}/{entity_type}/{uuid}` so a listing
//!   of the root directory gives a coarse audit trail by entity.
//! * All operations use `tokio::fs` so the Rocket reactor is never
//!   blocked.

use std::path::{Path, PathBuf};

/// Filesystem-backed attachment storage.
#[derive(Debug, Clone)]
pub struct LocalAttachmentStorage {
    pub storage_path: PathBuf,
}

impl LocalAttachmentStorage {
    pub fn new(storage_path: impl Into<PathBuf>) -> Self {
        Self {
            storage_path: storage_path.into(),
        }
    }

    /// Compute the safe on-disk path for a given `(entity_type, stored_filename)`.
    ///
    /// * `entity_type` must match the character set `[a-z_]+` — it is
    ///   a server-generated enum string, never user input, but we still
    ///   validate here so a path-traversal regression in a caller can't
    ///   silently escape the root.
    /// * `stored_filename` is expected to be a UUID (no extension, no
    ///   `/`, no `..`).
    pub fn resolve_path(
        &self,
        entity_type: &str,
        stored_filename: &str,
    ) -> Result<PathBuf, std::io::Error> {
        if entity_type.is_empty()
            || !entity_type
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_')
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid entity_type",
            ));
        }
        if stored_filename.is_empty()
            || stored_filename.contains('/')
            || stored_filename.contains('\\')
            || stored_filename.contains("..")
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid stored_filename",
            ));
        }
        Ok(self.storage_path.join(entity_type).join(stored_filename))
    }

    /// Write bytes to disk. Creates the parent directory if needed.
    pub async fn store_bytes(
        &self,
        entity_type: &str,
        stored_filename: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, std::io::Error> {
        let path = self.resolve_path(entity_type, stored_filename)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, bytes).await?;
        Ok(path)
    }

    /// Read the full file into memory. Callers should have already
    /// enforced a max size; this method does not re-check.
    pub async fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, std::io::Error> {
        tokio::fs::read(path).await
    }

    /// Delete a file. Missing files are treated as success (idempotent).
    pub async fn delete(&self, path: &Path) -> Result<(), std::io::Error> {
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_path_rejects_traversal_attempts() {
        let s = LocalAttachmentStorage::new("/data/attachments");

        assert!(s.resolve_path("journal", "../etc/passwd").is_err());
        assert!(s.resolve_path("journal", "abc/def").is_err());
        assert!(s.resolve_path("journal", "abc\\def").is_err());
        assert!(s.resolve_path("jour nal", "abc").is_err()); // space
        assert!(s.resolve_path("Journal", "abc").is_err()); // uppercase
        assert!(s.resolve_path("", "abc").is_err());
        assert!(s.resolve_path("journal", "").is_err());
    }

    #[test]
    fn resolve_path_accepts_safe_inputs() {
        let s = LocalAttachmentStorage::new("/data/attachments");
        let path = s
            .resolve_path("teaching_resource", "550e8400-e29b-41d4-a716-446655440000")
            .unwrap();
        assert!(path.starts_with("/data/attachments/teaching_resource/"));
        assert_eq!(
            path.file_name().unwrap().to_str().unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }
}
