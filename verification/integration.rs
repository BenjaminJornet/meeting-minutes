use crate::{archive_core::{ArchiveClient, read_metadata, write_metadata}, checkpoints::{Checkpoints, SummaryOptions},
    llm_client::LLMProvider, processor::generate_meeting_summary};
use serde_json::json;
use std::{fs, sync::{Arc, Mutex}};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};

async fn server(responses: Vec<(u16, Vec<u8>)>) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let url = format!("http://localhost:{}",listener.local_addr().unwrap().port());
    let requests = Arc::new(Mutex::new(Vec::new())); let saved = requests.clone();
    let task = tokio::spawn(async move {
        for (status, body) in responses {
            let (mut socket, _) = tokio::time::timeout(std::time::Duration::from_secs(15), listener.accept()).await.expect("mock server request timeout").unwrap();
            let mut input=Vec::new(); let mut buf=[0;8192];
            loop {
                let n=socket.read(&mut buf).await.unwrap(); if n==0 { break; } input.extend_from_slice(&buf[..n]);
                if let Some(end)=input.windows(4).position(|w| w==b"\r\n\r\n") {
                    let header=String::from_utf8_lossy(&input[..end]);
                    let size=header.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").and_then(|s| s.trim().parse::<usize>().ok())).unwrap_or(0);
                    if input.len()>=end+4+size { break; }
                }
            }
            saved.lock().unwrap().push(String::from_utf8_lossy(&input).into_owned());
            let header=format!("HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
            socket.write_all(header.as_bytes()).await.unwrap(); socket.write_all(&body).await.unwrap();
        }
    });
    (url,requests,task)
}

fn mp4() -> Vec<u8> { [b"\0\0\0\x10ftypisom\0\0\0\0".as_slice(),b"\0\0\0\x08moov",b"\0\0\0\x0cmdatDATA"].concat() }
fn llm(content: &str) -> (u16,Vec<u8>) { (200,json!({"done":true,"done_reason":"stop","message":{"content":content}}).to_string().into_bytes()) }
async fn checkpoints() -> Checkpoints {
    let pool=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::raw_sql(include_str!("../frontend/src-tauri/migrations/20261026000000_summary_profiles_and_checkpoints.sql")).execute(&pool).await.unwrap();
    Checkpoints {pool,meeting_id:"test".into(),run_key:"run".into()}
}

#[tokio::test]
async fn archive_checks_roundtrip_before_removing_every_audio_copy() {
    let dir=tempfile::tempdir().unwrap(); let audio=mp4();
    fs::write(dir.path().join("audio.mp4"),&audio).unwrap();
    fs::write(dir.path().join("audio.temp.wav"),b"temp").unwrap();
    fs::create_dir(dir.path().join(".checkpoints")).unwrap();
    fs::write(dir.path().join(".checkpoints/chunk.mp4"),b"temp").unwrap();
    fs::write(dir.path().join("transcripts.json"),b"[]").unwrap();
    write_metadata(dir.path(),&json!({"status":"completed"})).unwrap();
    let (url,requests,task)=server(vec![(200,b"accepted".to_vec()),(200,audio.clone()),(200,audio)]).await;
    let client=ArchiveClient::new(url.clone(),url,"test".into()).unwrap();
    client.archive(dir.path()).await.unwrap();
    assert!(crate::meeting_files::audio_inventory(dir.path()).unwrap().is_empty());
    assert!(dir.path().join("transcripts.json").exists());
    assert_eq!(read_metadata(dir.path()).unwrap()["archive_state"],"archived");
    client.restore(dir.path()).await.unwrap();
    assert_eq!(fs::read(dir.path().join("audio.mp4")).unwrap(),mp4());
    task.await.unwrap(); assert_eq!(requests.lock().unwrap().len(),3);
}

#[tokio::test]
async fn false_success_or_hash_mismatch_preserves_all_originals() {
    for remote in [b"{\"error\":\"not found\"}".to_vec(), {let mut a=mp4(); *a.last_mut().unwrap()=b'X'; a}] {
        let dir=tempfile::tempdir().unwrap(); fs::write(dir.path().join("audio.mp4"),mp4()).unwrap();
        fs::write(dir.path().join("audio.temp.wav"),b"temp").unwrap();
        write_metadata(dir.path(),&json!({"status":"completed"})).unwrap();
        let (url,_,task)=server(vec![(200,b"ok".to_vec()),(200,remote)]).await;
        assert!(ArchiveClient::new(url.clone(),url,"test".into()).unwrap().archive(dir.path()).await.is_err());
        assert!(dir.path().join("audio.mp4").exists()); assert!(dir.path().join("audio.temp.wav").exists());
        assert_ne!(read_metadata(dir.path()).unwrap()["is_archived"],true); task.await.unwrap();
    }
}

#[tokio::test]
async fn legacy_cleanup_and_interrupted_cleanup_are_idempotent() {
    let dir=tempfile::tempdir().unwrap(); fs::write(dir.path().join("audio.temp.wav"),b"temp").unwrap();
    write_metadata(dir.path(),&json!({"is_archived":true})).unwrap();
    let valid = decodable_mp4();
    let (url,requests,task)=server(vec![(200,valid.clone()),(200,valid)]).await;
    let client=ArchiveClient::new(url.clone(),url,"test".into()).unwrap(); client.archive(dir.path()).await.unwrap();
    let mut meta=read_metadata(dir.path()).unwrap(); meta["archive_state"]=json!("cleanup_pending"); meta["is_archived"]=json!(false);
    write_metadata(dir.path(),&meta).unwrap(); fs::write(dir.path().join("leftover.wav"),b"remaining").unwrap();
    client.archive(dir.path()).await.unwrap(); task.await.unwrap();
    assert!(crate::meeting_files::audio_inventory(dir.path()).unwrap().is_empty());
    assert!(requests.lock().unwrap().iter().all(|r| !r.contains("multipart/form-data")));
}

#[tokio::test]
async fn multi_chunk_generation_resumes_cached_steps_and_sets_ollama_options() {
    let cp=checkpoints().await; let options=SummaryOptions{context_tokens:8192,max_chunk_chars:1000,output_tokens:512};
    let text=(0..150).map(|i| format!("[00:12:34] Speaker A : Décision {i} à conserver vendredi.\n")).collect::<String>();
    let count=crate::processor::chunk_transcript(&text,1000,1000).len();
    // First chunk succeeds, second fails 3 times; retry must reuse the first checkpoint.
    let mut responses=vec![llm("[source=1] Décision 0, Speaker A, vendredi.")];
    responses.extend((0..3).map(|_| (503,b"unavailable".to_vec())));
    let (url,requests,task)=server(responses).await;
    let client=reqwest::Client::new();
    assert!(generate_meeting_summary(&client,&LLMProvider::Ollama,"test","",&text,"Respecter les dates","standard_meeting",&options,Some(&url),&cp).await.is_err());
    task.await.unwrap(); assert_eq!(requests.lock().unwrap().len(),4);
    // Keep the same run fingerprint as an explicit retry against the recovered endpoint.
    let mut responses=(1..count).map(|i| llm(&format!("[source={}] Speaker A : décision, vendredi.",i+1))).collect::<Vec<_>>();
    responses.push(llm("# Réunion\nDécisions d’Speaker A vendredi."));
    let (url,requests,task)=server(responses).await;
    let result=generate_meeting_summary(&client,&LLMProvider::Ollama,"test","",&text,"Respecter les dates","standard_meeting",&options,Some(&url),&cp).await.unwrap();
    assert_eq!(result.1,count as i64); task.await.unwrap();
    let requests=requests.lock().unwrap(); assert_eq!(requests.len(),count);
    assert!(requests.iter().all(|r| r.contains("/api/chat") && r.contains("\"think\":false") && r.contains("\"num_ctx\":8192") && r.contains("Respecter les dates")));
    let final_source=requests.last().unwrap(); assert!(final_source.contains("source=1"));
}

#[tokio::test]
async fn detects_truncated_and_reasoning_only_outputs() {
    for body in [json!({"done":true,"done_reason":"length","message":{"content":"partial"}}),
        json!({"done":true,"done_reason":"stop","message":{"content":"<think>only reasoning</think>"}})] {
        let (url,_,task)=server(vec![(200,body.to_string().into_bytes())]).await;
        let result=crate::llm_client::generate_summary(&reqwest::Client::new(),&LLMProvider::Ollama,"test","","system","user",Some(&url),&SummaryOptions::default()).await;
        assert!(result.is_err()); task.await.unwrap();
    }
}

#[tokio::test]
async fn profiles_are_isolated_and_checkpoint_input_changes_invalidate_cache() {
    let cp=checkpoints().await; let options=SummaryOptions{context_tokens:8192,max_chunk_chars:50_000,output_tokens:1024};
    crate::checkpoints::save_options(&cp.pool,"server-a","ollama","qwen",&options).await.unwrap();
    assert_eq!(crate::checkpoints::load_options(&cp.pool,"server-a","ollama","qwen").await.unwrap().context_tokens,8192);
    assert_eq!(crate::checkpoints::load_options(&cp.pool,"server-a","ollama","qwen").await.unwrap().max_chunk_chars,50_000);
    assert_eq!(crate::checkpoints::load_options(&cp.pool,"server-b","ollama","qwen").await.unwrap().context_tokens,32768);
    cp.put("analysis-1","input","output").await.unwrap(); assert_eq!(cp.get("analysis-1","input").await.unwrap(),Some("output".into()));
    assert!(cp.get("analysis-1","changed").await.unwrap().is_none());
}

#[tokio::test]
async fn default_thirty_thousand_character_budget_covers_a_long_meeting() {
    let cp = checkpoints().await;
    let options = SummaryOptions::default();
    let unsaved = crate::checkpoints::load_options(&cp.pool, "server", "ollama", "qwen").await.unwrap();
    assert_eq!(unsaved.max_chunk_chars, 30_000);
    let text = (0..2500).map(|i| format!("[01:12:34] Speaker A : SOURCE_SENTINEL_{i} vendredi.\n")).collect::<String>();
    assert!(text.chars().count() > 100_000);
    let count = crate::processor::chunk_transcript(&text, options.max_chunk_chars, 1000).len();
    assert!(count >= 4);
    let mut responses = (0..count).map(|i| llm(&format!("[source={}] Speaker A, vendredi.", i + 1))).collect::<Vec<_>>();
    responses.push(llm("# Réunion\nDécisions consolidées."));
    let (url, requests, task) = server(responses).await;
    let result = generate_meeting_summary(&reqwest::Client::new(), &LLMProvider::Ollama, "test", "",
        &text, "Conserver les chiffres", "standard_meeting", &options, Some(&url), &cp).await.unwrap();
    task.await.unwrap();
    assert_eq!(result.1, count as i64);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), count + 1);
    let sources = requests[..count].iter().map(|request| {
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(parsed["options"]["num_ctx"], 32768);
        assert_eq!(parsed["options"]["num_predict"], 4096);
        assert_eq!(parsed["think"], false);
        let source = parsed["messages"][1]["content"].as_str().unwrap().splitn(3, '\n').nth(2).unwrap().to_owned();
        assert!(source.chars().count() <= 30_000);
        source
    }).collect::<Vec<_>>();
    for i in 0..2500 {
        assert!(sources.iter().any(|source| source.contains(&format!("SOURCE_SENTINEL_{i} "))), "missing source {i}");
    }
}

// Explicit opt-in: synthetic content only, never run against a real server in the default suite.
#[tokio::test]
#[ignore = "requires MEETILY_SMOKE_OLLAMA_URL and a real Ollama server"]
async fn real_ollama_long_summary_smoke() {
    let endpoint = std::env::var("MEETILY_SMOKE_OLLAMA_URL").expect("explicit server URL required");
    let cp = checkpoints().await;
    let filler = "[00:15:00] Speaker A : Nous passons en revue les points déjà traités, sans nouvelle décision.\n";
    let text = format!("[00:00:01] Speaker A : Le projet se nomme SYNTHETIC_PROJECT. Budget proposé : 42 000 euros.\n{}\n\
        [01:00:00] Speaker B : Correction : le budget de 42 000 euros est abandonné. Le budget final validé est de 47 250 euros.\n\
        [01:00:10] Speaker A : Speaker C préparera le dossier SYNTHETIC_PROJECT pour le 30 novembre 2030.\n{}\n\
        [02:00:00] Speaker A : Nous confirmons la décision finale : SYNTHETIC_PROJECT, 47 250 euros, responsable Speaker C, échéance 30 novembre 2030.\n",
        filler.repeat(650), filler.repeat(650));
    assert!(text.chars().count() > 100_000);
    let started = std::time::Instant::now();
    let client = reqwest::Client::builder().connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(1200)).build().unwrap();
    let (markdown, chunks) = generate_meeting_summary(&client, &LLMProvider::Ollama, "qwen3.5:9b", "",
        &text, "Rédiger en français. Conserver le nom du projet, le budget final, sa correction, la responsable et l’échéance. Les répétitions n’ajoutent aucune décision.",
        "standard_meeting", &SummaryOptions::default(), Some(&endpoint), &cp).await.unwrap();
    println!("chars={} | chunks={chunks} | seconds={:.1}\n{markdown}", text.chars().count(), started.elapsed().as_secs_f64());
    assert!(chunks >= 4);
    let folded = markdown.to_lowercase();
    assert!(folded.contains("synthetic_project"));
    assert!(folded.contains("speaker c"));
    assert!(!folded.contains("conformité"), "compliance duties were never assigned");
    assert!(!markdown.lines().any(|line| line.trim_start().starts_with("| Speaker B |")),
        "announcing the budget decision must not create a task assigned to Speaker B");
    let compact = markdown.chars().filter(|c| !c.is_whitespace() && !matches!(c, ',' | '.' | '*')).collect::<String>();
    assert!(compact.contains("47250"), "final budget absent from the report");
    let (stage,): (String,) = sqlx::query_as("SELECT stage FROM summary_progress WHERE meeting_id='test'")
        .fetch_one(&cp.pool).await.unwrap();
    assert_eq!(stage, "completed");
}

#[tokio::test]
async fn hundred_thousand_characters_require_recursive_consolidation_without_gaps() {
    let cp = checkpoints().await;
    let options = SummaryOptions { context_tokens: 8192, max_chunk_chars: 1000, output_tokens: 1024 };
    let text = (0..2500).map(|i| format!("[01:12:34] Speaker A : SOURCE_SENTINEL_{i} vendredi.\n")).collect::<String>();
    assert!(text.chars().count() > 100_000);
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let url = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    let task = tokio::spawn(async move {
        let mut analyses = Vec::new();
        let mut reductions = 0;
        loop {
            let (mut socket, _) = tokio::time::timeout(std::time::Duration::from_secs(15), listener.accept()).await.expect("mock server request timeout").unwrap();
            let mut input = Vec::new(); let mut buffer = [0; 8192];
            let (end, size) = loop {
                let n = socket.read(&mut buffer).await.unwrap(); assert_ne!(n, 0);
                input.extend_from_slice(&buffer[..n]);
                if let Some(end) = input.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&input[..end]);
                    let size = header.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").and_then(|s| s.trim().parse::<usize>().ok())).unwrap();
                    if input.len() >= end + 4 + size { break (end, size); }
                }
            };
            let request: serde_json::Value = serde_json::from_slice(&input[end+4..end+4+size]).unwrap();
            let system = request["messages"][0]["content"].as_str().unwrap();
            let user = request["messages"][1]["content"].as_str().unwrap();
            assert!(system.contains("Conserver les chiffres"));
            let final_step = system.contains("expert meeting secretary");
            let content = if final_step { "# Réunion\nDécision finale.".to_string() }
                else if system.contains("extract detailed") {
                    analyses.push(user.to_owned());
                    "fait précis. ".repeat(180)
                } else { reductions += 1; "fait consolidé. ".repeat(120) };
            let body = llm(&content).1;
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await.unwrap();
            socket.write_all(&body).await.unwrap();
            if final_step {
                for i in 0..2500 {
                    assert!(analyses.iter().any(|s| s.contains(&format!("SOURCE_SENTINEL_{i} "))), "missing source {i}");
                }
                assert!(reductions > 1);
                break;
            }
        }
        reductions
    });
    let result = generate_meeting_summary(&reqwest::Client::new(), &LLMProvider::Ollama, "test", "",
        &text, "Conserver les chiffres", "standard_meeting", &options, Some(&url), &cp).await.unwrap();
    assert!(result.1 > 100);
    assert!(task.await.unwrap() > 1);
    let levels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM summary_checkpoints WHERE step_key LIKE 'reduce-1-%'")
        .fetch_one(&cp.pool).await.unwrap();
    assert!(levels.0 > 0, "must require at least two consolidation levels");
}


#[tokio::test]
async fn archive_upload_failure_and_invalid_restore_preserve_local_state() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("audio.mp4"), mp4()).unwrap();
    write_metadata(dir.path(), &json!({"status":"completed"})).unwrap();
    let (url, _, task) = server(vec![(503, b"unavailable".to_vec())]).await;
    let client = ArchiveClient::new(url.clone(), url, "test".into()).unwrap();
    assert!(client.archive(dir.path()).await.unwrap_err().starts_with("archive_upload_failed:"));
    assert_eq!(fs::read(dir.path().join("audio.mp4")).unwrap(), mp4());
    task.await.unwrap();
    fs::remove_file(dir.path().join("audio.mp4")).unwrap();
    for bytes in [b"<html>missing</html>".to_vec(), mp4()[..30].to_vec()] {
        let (url, _, task) = server(vec![(200, bytes)]).await;
        let client = ArchiveClient::new(url.clone(), url, "test".into()).unwrap();
        assert!(client.restore(dir.path()).await.is_err());
        assert!(crate::meeting_files::audio_inventory(dir.path()).unwrap().is_empty());
        task.await.unwrap();
    }
}

#[tokio::test]
async fn archive_refuses_active_conversion_or_unfinished_import() {
    let dir = tempfile::tempdir().unwrap();
    write_metadata(dir.path(), &json!({"status":"processing"})).unwrap();
    let client = ArchiveClient::new("http://localhost:1".into(), "http://localhost:1".into(), "test".into()).unwrap();
    let operation = crate::meeting_files::MeetingOperation::acquire(dir.path()).unwrap();
    assert!(client.archive(dir.path()).await.unwrap_err().starts_with("meeting_busy:"));
    drop(operation);
    assert!(client.archive(dir.path()).await.unwrap_err().starts_with("meeting_busy:"));
}

#[tokio::test]
async fn failed_cleanup_retains_durable_remote_proof_and_can_resume() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("audio.mp4"), mp4()).unwrap();
    let checkpoint = dir.path().join(".checkpoints");
    fs::create_dir(&checkpoint).unwrap();
    fs::write(checkpoint.join("leftover.wav"), b"temp").unwrap();
    fs::set_permissions(&checkpoint, fs::Permissions::from_mode(0o500)).unwrap();
    write_metadata(dir.path(), &json!({"status":"completed"})).unwrap();
    let (url, requests, task) = server(vec![(200, b"ok".to_vec()), (200, mp4()), (200, mp4())]).await;
    let client = ArchiveClient::new(url.clone(), url, "test".into()).unwrap();
    let result = client.archive(dir.path()).await;
    fs::set_permissions(&checkpoint, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.unwrap_err().contains("audio_cleanup_failed"), "run this test as an unprivileged user");
    let metadata = read_metadata(dir.path()).unwrap();
    assert_eq!(metadata["archive_state"], "cleanup_pending");
    assert_eq!(metadata["is_archived"], false);
    assert!(metadata["archive_proof"]["sha256"].is_string());
    client.archive(dir.path()).await.unwrap();
    assert!(crate::meeting_files::audio_inventory(dir.path()).unwrap().is_empty());
    task.await.unwrap();
    assert_eq!(requests.lock().unwrap().len(), 3);
}

fn decodable_mp4() -> Vec<u8> {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("tone.mp4");
    let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=0.1", "-c:a", "aac", "-movflags", "+faststart"])
        .arg(&file).status().expect("FFmpeg required inside the verification container");
    assert!(status.success());
    fs::read(file).unwrap()
}

#[tokio::test]
async fn legacy_archive_with_only_fake_mp4_boxes_preserves_residuals() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("audio.temp.wav"), b"keep").unwrap();
    write_metadata(dir.path(), &json!({"is_archived":true})).unwrap();
    let (url, _, task) = server(vec![(200, mp4())]).await;
    let client = ArchiveClient::new(url.clone(), url, "test".into()).unwrap();
    assert!(client.archive(dir.path()).await.is_err());
    assert!(dir.path().join("audio.temp.wav").exists());
    task.await.unwrap();
}
