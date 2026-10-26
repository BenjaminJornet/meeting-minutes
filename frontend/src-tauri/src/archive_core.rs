use crate::meeting_files::{audio_inventory, clean_audio, MeetingOperation};
use chrono::Utc;
use futures_util::{stream, StreamExt};
use reqwest::{multipart::{Form, Part}, Client};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::{self, File}, io::{Read, Write}, path::{Path, PathBuf}, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioProof { pub bytes: u64, pub sha256: String }

pub fn read_metadata(folder: &Path) -> Result<Value, String> {
    let value: Value = serde_json::from_slice(&fs::read(folder.join("metadata.json")).map_err(|e| e.to_string())?)
        .map_err(|e| format!("metadata_invalid: {e}"))?;
    if !value.is_object() { return Err("metadata_invalid: objet attendu".into()); }
    Ok(value)
}

pub fn write_metadata(folder: &Path, metadata: &Value) -> Result<(), String> {
    let mut temp = tempfile::NamedTempFile::new_in(folder).map_err(|e| e.to_string())?;
    temp.write_all(&serde_json::to_vec_pretty(metadata).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(folder.join("metadata.json")).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    File::open(folder).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn fingerprint(path: &Path) -> Result<AudioProof, String> {
    if fs::symlink_metadata(path).map_err(|e| e.to_string())?.file_type().is_symlink() {
        return Err("audio_invalid: lien symbolique interdit".into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new(); let mut bytes = 0; let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        hash.update(&buffer[..n]); bytes += n as u64;
    }
    if bytes == 0 { return Err("audio_invalid: fichier vide".into()); }
    Ok(AudioProof { bytes, sha256: format!("{:x}", hash.finalize()) })
}

/// Explicit manual cleanup is local only and does not assert remote availability.
pub fn cleanup_archived_audio(folder: &Path) -> Result<(), String> {
    let _guard = MeetingOperation::acquire(folder)?;
    let mut metadata = read_metadata(folder)?;
    // Historical status can survive a crash; the operation lock tracks live work.
    if metadata["is_archived"] != true && metadata["archive_state"] != "cleanup_pending" {
        return Err("audio_cleanup_not_archived: seuls les audios des réunions archivées peuvent être nettoyés".into());
    }
    metadata["archive_state"] = json!("cleanup_pending");
    metadata["audio_cleanup_mode"] = json!("local_only");
    write_metadata(folder, &metadata)?;
    clean_audio(folder)?;
    metadata["archive_state"] = json!("archived");
    metadata["is_archived"] = json!(true);
    metadata["audio_cleaned_at"] = json!(Utc::now().to_rfc3339());
    write_metadata(folder, &metadata)
}

/// Validate top-level ISO BMFF framing without retaining the recording in memory.
#[derive(Default)]
struct Mp4Validator {
    header: Vec<u8>, remaining: u64, until_eof: bool, ftyp: bool, moov: bool, mdat: bool,
}
impl Mp4Validator {
    fn push(&mut self, mut data: &[u8]) -> Result<(), String> {
        while !data.is_empty() {
            if self.until_eof { return Ok(()); }
            if self.remaining > 0 {
                let n = self.remaining.min(data.len() as u64) as usize;
                self.remaining -= n as u64; data = &data[n..]; continue;
            }
            let wanted = if self.header.len() >= 8 && self.header[..4] == [0,0,0,1] { 16 } else { 8 };
            let n = (wanted - self.header.len()).min(data.len());
            self.header.extend_from_slice(&data[..n]); data = &data[n..];
            if self.header.len() < wanted { continue; }
            let short = u32::from_be_bytes(self.header[..4].try_into().unwrap());
            if short == 1 && wanted == 8 { continue; }
            let size = if short == 1 { u64::from_be_bytes(self.header[8..16].try_into().unwrap()) } else { short as u64 };
            match &self.header[4..8] {
                b"ftyp" => self.ftyp = true,
                b"moov" => self.moov = true,
                b"mdat" => self.mdat = true,
                _ => (),
            }
            if size == 0 { self.until_eof = true; }
            else if size < wanted as u64 { return Err("archive_invalid_audio: taille de conteneur invalide".into()); }
            else { self.remaining = size - wanted as u64; }
            self.header.clear();
        }
        Ok(())
    }
    fn finish(&self) -> Result<(), String> {
        if self.ftyp && self.moov && self.mdat && self.header.is_empty() && self.remaining == 0 { Ok(()) }
        else { Err("archive_invalid_audio: l’archive distante n’est pas un MP4 complet".into()) }
    }
}

const LEGACY_AUDIO_MEMORY_LIMIT: usize = 512 * 1024 * 1024;

fn append_legacy_audio(audio: &mut Vec<u8>, chunk: &[u8]) -> Result<(), String> {
    append_bounded_audio(audio, chunk, LEGACY_AUDIO_MEMORY_LIMIT)
}

fn append_bounded_audio(audio: &mut Vec<u8>, chunk: &[u8], limit: usize) -> Result<(), String> {
    if chunk.len() > limit.saturating_sub(audio.len()) {
        return Err("archive_verification_memory_limit: ancienne archive trop volumineuse pour la vérification en mémoire (512 Mo maximum) ; fichiers locaux conservés".into());
    }
    let required = audio.len() + chunk.len();
    if required > audio.capacity() {
        let capacity = required.max(audio.capacity().saturating_mul(2).min(limit));
        audio.try_reserve_exact(capacity - audio.len()).map_err(|_| "archive_verification_memory_limit: mémoire insuffisante ; fichiers locaux conservés")?;
    }
    audio.extend_from_slice(chunk);
    Ok(())
}

/// FFmpeg seeks through a private loopback endpoint backed only by RAM.
/// Aborting or completing verification releases the listener and audio buffer.
struct MemoryAudioServer(tokio::task::JoinHandle<()>);
impl Drop for MemoryAudioServer {
    fn drop(&mut self) { self.0.abort(); }
}

fn audio_range(header: &str, len: usize) -> Option<(usize, usize, bool)> {
    if len == 0 { return None; }
    let range = header.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case("range").then_some(value.trim())
    });
    match range {
        None => Some((0, len - 1, false)),
        Some(value) => {
            let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
            let start: usize = start.parse().ok()?;
            let end = if end.is_empty() { len - 1 } else { end.parse::<usize>().ok()?.min(len - 1) };
            (start <= end && start < len).then_some((start, end, true))
        }
    }
}

async fn serve_memory_audio(mut socket: tokio::net::TcpStream, audio: &[u8], path: &str) -> std::io::Result<()> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 8192 { return Ok(()); }
        header.push(socket.read_u8().await?);
    }
    let header = String::from_utf8_lossy(&header);
    if header.lines().next() != Some(&format!("GET {path} HTTP/1.1")) {
        socket.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await?;
        return Ok(());
    }
    let Some((start, end, partial)) = audio_range(&header, audio.len()) else {
        socket.write_all(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await?;
        return Ok(());
    };
    let status = if partial { "206 Partial Content" } else { "200 OK" };
    let range = if partial { format!("Content-Range: bytes {start}-{end}/{}\r\n", audio.len()) } else { String::new() };
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: audio/mp4\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\n{range}Connection: close\r\n\r\n", end - start + 1);
    socket.write_all(response.as_bytes()).await?;
    socket.write_all(&audio[start..=end]).await
}

async fn decode_seekable_audio(decoder: &Path, audio: Vec<u8>) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await
        .map_err(|_| "archive_verification_unavailable: lecture en mémoire indisponible")?;
    let address = listener.local_addr().map_err(|_| "archive_verification_unavailable: adresse locale indisponible")?;
    let path = format!("/{:032x}/audio.mp4", rand::random::<u128>());
    let url = format!("http://{address}{path}");
    let _server = MemoryAudioServer(tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            // FFmpeg may close an earlier range when it seeks elsewhere.
            let _ = tokio::time::timeout(Duration::from_secs(7200), serve_memory_audio(socket, &audio, &path)).await;
        }
    }));
    let mut child = tokio::process::Command::new(decoder)
        .args(["-nostdin", "-v", "error", "-xerror", "-http_proxy", "", "-i", &url, "-map", "0:a:0", "-f", "null", "-"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true)
        .spawn().map_err(|_| "archive_decoder_unavailable: FFmpeg requis pour vérifier cette ancienne archive")?;
    let status = tokio::time::timeout(Duration::from_secs(7200), child.wait()).await
        .map_err(|_| "archive_verify_failed: délai de décodage dépassé")?
        .map_err(|_| "archive_invalid_audio: décodage interrompu")?;
    if !status.success() { return Err("archive_invalid_audio: l’archive ne contient pas d’audio décodable ; fichiers locaux conservés".into()); }
    Ok(())
}

pub struct ArchiveClient { client: Client, upload_url: String, restore_url: String, api_key: String, decoder: PathBuf }
impl ArchiveClient {
    pub fn new(upload_url: String, restore_url: String, api_key: String) -> Result<Self, String> {
        let client = Client::builder().connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(7200)).redirect(reqwest::redirect::Policy::none())
            .build().map_err(|e| e.to_string())?;
        let decoder = std::env::var_os("MEETILY_FFMPEG_PATH").map(PathBuf::from).unwrap_or_else(|| {
            ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"].iter()
                .map(PathBuf::from).find(|p| p.is_file()).unwrap_or_else(|| PathBuf::from("ffmpeg"))
        });
        Ok(Self { client, upload_url, restore_url, api_key, decoder })
    }
    pub fn from_env() -> Result<Self, String> {
        Self::new(std::env::var("MEETILY_ARCHIVE_WEBHOOK_URL").map_err(|_| "MEETILY_ARCHIVE_WEBHOOK_URL absent")?,
            std::env::var("MEETILY_RESTORE_WEBHOOK_URL").map_err(|_| "MEETILY_RESTORE_WEBHOOK_URL absent")?,
            std::env::var("MEETILY_N8N_X_API_KEY").map_err(|_| "MEETILY_N8N_X_API_KEY absent")?)
    }
    async fn upload(&self, folder_name: &str, path: &Path, bytes: u64) -> Result<(), String> {
        let file = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
        let chunks = stream::try_unfold(file, |mut file| async {
            let mut buffer = vec![0; 65536]; let n = file.read(&mut buffer).await?;
            buffer.truncate(n);
            Ok::<_, std::io::Error>(if n == 0 { None } else { Some((buffer, file)) })
        });
        let part = Part::stream_with_length(reqwest::Body::wrap_stream(chunks), bytes)
            .file_name("audio.mp4").mime_str("audio/mp4").map_err(|e| e.to_string())?;
        let response = self.client.post(&self.upload_url).header("X-API-Key", &self.api_key)
            .multipart(Form::new().text("folder", folder_name.to_string()).part("file", part))
            .send().await.map_err(|_| "archive_upload_failed: webhook inaccessible")?;
        if !response.status().is_success() { return Err(format!("archive_upload_failed: HTTP {}", response.status())); }
        Ok(())
    }
    async fn download(&self, folder_name: &str, mut destination: Option<&mut File>, decode: bool) -> Result<AudioProof, String> {
        // Legacy MP4 indexes may follow the media: decoding requires seeking.
        // Buffer only legacy verification in RAM, with a strict allocation limit.
        let mut legacy_audio = if decode { Some(Vec::new()) } else { None };
        let response = self.client.post(&self.restore_url).header("X-API-Key", &self.api_key)
            .json(&json!({"folder": folder_name, "file": "audio.mp4"})).send().await
            .map_err(|_| "archive_verify_failed: restauration inaccessible")?;
        if !response.status().is_success() { return Err(format!("archive_verify_failed: HTTP {}", response.status())); }
        let expected_length = response.content_length(); let mut bytes = 0u64;
        let mut hash = Sha256::new(); let mut validator = Mp4Validator::default();
        let mut chunks = response.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(|_| "archive_verify_failed: transfert interrompu")?;
            validator.push(&chunk)?; hash.update(&chunk); bytes += chunk.len() as u64;
            if let Some(audio) = legacy_audio.as_mut() {
                append_legacy_audio(audio, &chunk)?;
            }
            if let Some(file) = destination.as_deref_mut() { file.write_all(&chunk).map_err(|e| e.to_string())?; }
        }
        if expected_length.is_some_and(|n| n != bytes) { return Err("archive_verify_failed: transfert incomplet".into()); }
        validator.finish()?;
        if let Some(audio) = legacy_audio {
            decode_seekable_audio(&self.decoder, audio).await?;
        }
        Ok(AudioProof { bytes, sha256: format!("{:x}", hash.finalize()) })
    }

    pub async fn archive(&self, folder: &Path) -> Result<(), String> {
        let _guard = MeetingOperation::acquire(folder)?;
        let mut metadata = read_metadata(folder)?;
        if matches!(metadata["status"].as_str(), Some("recording" | "processing")) { return Err("meeting_busy: enregistrement non finalisé".into()); }
        let folder_name = remote_folder_name(folder, &metadata)?;
        let original = folder.join("audio.mp4");
        let local = if original.exists() { Some(fingerprint(&original)?) } else { None };
        let previous_proof = metadata.get("archive_proof").cloned().and_then(|v| serde_json::from_value::<AudioProof>(v).ok());
        let was_archived = metadata["is_archived"].as_bool().unwrap_or(false);
        // Retry cleanup without uploading again; always reverify remote storage.
        if metadata["archive_state"] != "cleanup_pending" && !was_archived {
            let proof = local.as_ref().ok_or("archive_audio_missing: audio final absent")?;
            self.upload(&folder_name, &original, proof.bytes).await?;
        }
        let remote = self.download(&folder_name, None, local.is_none() && previous_proof.is_none()).await?;
        if let Some(expected) = local.as_ref().or(previous_proof.as_ref()) {
            if expected != &remote { return Err("archive_verify_failed: empreinte ou taille différente".into()); }
        }
        metadata["archive_proof"] = json!(remote);
        metadata["remote_folder_name"] = json!(folder_name);
        metadata["archive_verified_at"] = json!(Utc::now().to_rfc3339());
        metadata["archive_state"] = json!("cleanup_pending");
        metadata["is_archived"] = json!(false);
        write_metadata(folder, &metadata)?;
        // No deletion occurs before the durable remote proof above.
        clean_audio(folder)?;
        metadata["archive_state"] = json!("archived");
        metadata["is_archived"] = json!(true);
        if metadata.get("archived_at").is_none() { metadata["archived_at"] = json!(Utc::now().to_rfc3339()); }
        write_metadata(folder, &metadata)
    }

    pub async fn restore(&self, folder: &Path) -> Result<(), String> {
        let _guard = MeetingOperation::acquire(folder)?;
        let mut metadata = read_metadata(folder)?;
        let destination = folder.join("audio.mp4");
        if destination.exists() { return Err("restore_audio_exists: audio local déjà présent".into()); }
        // Named .mp4 temp is included in audio inventory after a crash.
        let mut temp = tempfile::Builder::new().prefix(".restore-").suffix(".mp4").tempfile_in(folder).map_err(|e| e.to_string())?;
        let proof = self.download(&remote_folder_name(folder, &metadata)?, Some(temp.as_file_mut()), metadata.get("archive_proof").is_none()).await?;
        if let Some(expected) = metadata.get("archive_proof") {
            let expected: AudioProof = serde_json::from_value(expected.clone()).map_err(|e| e.to_string())?;
            if expected != proof { return Err("archive_verify_failed: empreinte de restauration différente".into()); }
        }
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist_noclobber(destination).map_err(|e| e.to_string())?;
        metadata["is_archived"] = json!(false); metadata["archive_state"] = json!("local");
        write_metadata(folder, &metadata)
    }
}

fn remote_folder_name(folder: &Path, metadata: &Value) -> Result<String, String> {
    metadata["remote_folder_name"].as_str().or_else(|| folder.file_name().and_then(|s| s.to_str()))
        .map(str::to_owned).ok_or_else(|| "meeting_path_invalid: nom de dossier absent".into())
}

#[derive(Serialize)]
pub struct ArchiveInventory { pub files: Vec<String>, pub bytes: u64 }
pub fn inventory(folder: &Path) -> Result<ArchiveInventory, String> {
    let files = audio_inventory(folder)?;
    let bytes = files.iter().try_fold(0u64, |n,p| fs::symlink_metadata(p).map(|m| n + if m.is_file() { m.len() } else { 0 }))
        .map_err(|e| e.to_string())?;
    Ok(ArchiveInventory { files: files.iter().filter_map(|p| p.strip_prefix(folder).ok()).map(|p| p.to_string_lossy().into()).collect(), bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn mp4() -> Vec<u8> {
        [b"\0\0\0\x10ftypisom\0\0\0\0".as_slice(), b"\0\0\0\x08moov", b"\0\0\0\x0cmdatDATA"].concat()
    }
    #[test]
    fn rejects_http_success_with_html_or_truncated_audio() {
        let mut v=Mp4Validator::default(); assert!(v.push(b"<html>error</html>").and_then(|_| v.finish()).is_err());
        let mut v=Mp4Validator::default(); let bytes=mp4();
        for b in bytes.chunks(3) { v.push(b).unwrap(); } v.finish().unwrap();
        let mut v=Mp4Validator::default(); v.push(&bytes[..bytes.len()-1]).unwrap(); assert!(v.finish().is_err());
    }
    #[test]
    fn metadata_updates_preserve_transcripts_and_unknown_fields() {
        let dir=tempfile::tempdir().unwrap(); let value=json!({"title":"Test", "custom":42});
        write_metadata(dir.path(), &value).unwrap(); assert_eq!(read_metadata(dir.path()).unwrap(),value);
    }

    #[test]
    fn manual_cleanup_needs_no_webhook_or_remote_proof_and_preserves_documents() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".checkpoints")).unwrap();
        for name in ["audio.mp4", "audio.temp.wav", ".checkpoints/chunk.m4a"] {
            fs::write(dir.path().join(name), b"local audio").unwrap();
        }
        for name in ["transcripts.json", "summary.md"] { fs::write(dir.path().join(name), b"keep").unwrap(); }
        write_metadata(dir.path(), &json!({"is_archived":true,"custom":"keep"})).unwrap();
        cleanup_archived_audio(dir.path()).unwrap();
        cleanup_archived_audio(dir.path()).unwrap();
        assert!(audio_inventory(dir.path()).unwrap().is_empty());
        for name in ["transcripts.json", "summary.md"] { assert_eq!(fs::read(dir.path().join(name)).unwrap(), b"keep"); }
        let metadata = read_metadata(dir.path()).unwrap();
        assert_eq!(metadata["custom"], "keep");
        assert_eq!(metadata["archive_state"], "archived");
        assert_eq!(metadata["audio_cleanup_mode"], "local_only");
        assert!(metadata.get("archive_proof").is_none());
        assert!(metadata.get("archive_verified_at").is_none());
    }

    #[test]
    fn manual_cleanup_refuses_active_or_unarchived_meetings() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("audio.mp4"), b"keep").unwrap();
        for metadata in [json!({"is_archived":false}), json!({"is_archived":false,"status":"recording"}), json!({"is_archived":false,"status":"processing"})] {
            write_metadata(dir.path(), &metadata).unwrap();
            assert!(cleanup_archived_audio(dir.path()).is_err());
            assert_eq!(read_metadata(dir.path()).unwrap(), metadata);
            assert_eq!(fs::read(dir.path().join("audio.mp4")).unwrap(), b"keep");
        }
        write_metadata(dir.path(), &json!({"is_archived":true,"status":"processing"})).unwrap();
        let guard = MeetingOperation::acquire(dir.path()).unwrap();
        assert!(cleanup_archived_audio(dir.path()).unwrap_err().starts_with("meeting_busy:"));
        assert_eq!(fs::read(dir.path().join("audio.mp4")).unwrap(), b"keep");
        drop(guard);
    }

    #[test]
    fn manual_cleanup_allows_stale_status_when_no_operation_holds_the_lock() {
        for status in ["recording", "processing"] {
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join("audio.temp.wav"), b"stale audio").unwrap();
            fs::write(dir.path().join("transcripts.json"), b"keep transcript").unwrap();
            fs::write(dir.path().join("summary.md"), b"keep summary").unwrap();
            write_metadata(dir.path(), &json!({"is_archived":true,"status":status})).unwrap();
            cleanup_archived_audio(dir.path()).unwrap();
            assert!(audio_inventory(dir.path()).unwrap().is_empty());
            assert_eq!(fs::read(dir.path().join("transcripts.json")).unwrap(), b"keep transcript");
            assert_eq!(fs::read(dir.path().join("summary.md")).unwrap(), b"keep summary");
            assert_eq!(read_metadata(dir.path()).unwrap()["archive_state"], "archived");
        }
    }

    #[test]
    fn legacy_memory_budget_refuses_growth_before_copying() {
        let mut audio = Vec::new();
        append_bounded_audio(&mut audio, b"1234", 5).unwrap();
        assert!(append_bounded_audio(&mut audio, b"56", 5).unwrap_err().starts_with("archive_verification_memory_limit:"));
        assert_eq!(audio, b"1234");
        append_bounded_audio(&mut audio, b"5", 5).unwrap();
        assert_eq!(audio, b"12345");
    }

    #[test]
    fn memory_audio_ranges_allow_seeking_and_reject_invalid_offsets() {
        assert_eq!(audio_range("GET / HTTP/1.1\r\nRange: bytes=40-\r\n\r\n", 100), Some((40, 99, true)));
        assert_eq!(audio_range("GET / HTTP/1.1\r\nrange: bytes=0-200\r\n\r\n", 100), Some((0, 99, true)));
        assert_eq!(audio_range("GET / HTTP/1.1\r\n\r\n", 100), Some((0, 99, false)));
        for range in ["100-", "20-10", "1-2,4-5", "oops", "-2"] {
            assert_eq!(audio_range(&format!("GET / HTTP/1.1\r\nRange: bytes={range}\r\n\r\n"), 100), None);
        }
        assert_eq!(audio_range("GET / HTTP/1.1\r\n\r\n", 0), None);
    }
}
