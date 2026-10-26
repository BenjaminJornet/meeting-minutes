//! Filesystem ownership shared by archiving, recording and retranscription.
use fs2::FileExt;
use std::{fs::{self, File, OpenOptions}, path::{Path, PathBuf}};

#[derive(Debug)]
pub struct MeetingOperation { _lock: File }

impl MeetingOperation {
    pub fn acquire(folder: &Path) -> Result<Self, String> {
        if fs::symlink_metadata(folder).map_err(|e| e.to_string())?.file_type().is_symlink() {
            return Err("meeting_path_invalid: dossier symbolique interdit".into());
        }
        let lock_path = folder.join(".audio-operation.lock");
        if fs::symlink_metadata(&lock_path).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
            return Err("meeting_path_invalid: verrou symbolique interdit".into());
        }
        let file = OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .open(lock_path).map_err(|e| e.to_string())?;
        file.try_lock_exclusive().map_err(|_| "meeting_busy: une opération audio est en cours".to_string())?;
        Ok(Self { _lock: file })
    }
}

impl Drop for MeetingOperation {
    fn drop(&mut self) { let _ = FileExt::unlock(&self._lock); }
}

pub fn is_audio(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).map(|s| matches!(s.to_ascii_lowercase().as_str(),
        "mp4" | "m4a" | "wav" | "mp3" | "flac" | "ogg" | "opus" | "webm" | "aac" | "pcm" | "aiff" | "aif" | "wma" | "caf"))
        .unwrap_or(false)
}

/// Enumerate owned audio recursively, including hidden checkpoints. Never follow links.
pub fn audio_inventory(folder: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            // Remove audio links themselves during cleanup; their targets are never read.
            if is_audio(&entry.path()) { files.push(entry.path()); }
        } else if kind.is_dir() {
            files.extend(audio_inventory(&entry.path())?);
        } else if kind.is_file() && is_audio(&entry.path()) { files.push(entry.path()); }
    }
    files.sort();
    Ok(files)
}

pub fn clean_audio(folder: &Path) -> Result<(), String> {
    for path in audio_inventory(folder)? { fs::remove_file(path).map_err(|e| format!("audio_cleanup_failed: {e}"))?; }
    if !audio_inventory(folder)?.is_empty() { return Err("audio_cleanup_failed: fichiers audio restants".into()); }
    Ok(())
}

/// Called only while holding the meeting operation lock, so no active converter is touched.
pub fn recover_temporary_audio(folder: &Path) -> Result<(), String> {
    let temp = folder.join(".audio-temp");
    if let Ok(meta) = fs::symlink_metadata(&temp) {
        if meta.file_type().is_symlink() { return Err("meeting_path_invalid: temporaires symboliques interdits".into()); }
        for entry in fs::read_dir(&temp).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_file() && is_audio(&entry.path()) {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
    }
    for entry in fs::read_dir(folder).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".restore-") && name.ends_with(".mp4") && entry.file_type().map_err(|e| e.to_string())?.is_file() {
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_concurrent_audio_operations_and_releases_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let guard = MeetingOperation::acquire(dir.path()).unwrap();
        assert!(MeetingOperation::acquire(dir.path()).is_err());
        drop(guard);
        assert!(MeetingOperation::acquire(dir.path()).is_ok());
    }
    #[test]
    fn cleans_hidden_audio_and_keeps_documents() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".checkpoints")).unwrap();
        fs::write(dir.path().join(".checkpoints/a.MP4"), b"audio").unwrap();
        fs::write(dir.path().join("audio.temp.wav"), b"audio").unwrap();
        fs::write(dir.path().join("transcripts.json"), b"[]").unwrap();
        clean_audio(dir.path()).unwrap();
        assert!(audio_inventory(dir.path()).unwrap().is_empty());
        assert!(dir.path().join("transcripts.json").exists());
    }
    #[cfg(unix)]
    #[test]
    fn never_follows_external_audio_or_directory_links() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap(); let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("audio.wav"), b"keep").unwrap();
        symlink(outside.path(), dir.path().join("outside")).unwrap();
        symlink(outside.path().join("audio.wav"), dir.path().join("linked.wav")).unwrap();
        clean_audio(dir.path()).unwrap();
        assert!(outside.path().join("audio.wav").exists());
        assert!(!dir.path().join("linked.wav").exists());
    }
}
