use std::collections::HashSet;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::task::spawn_blocking;

use rand::prelude::IndexedRandom;
use rand::rngs::{OsRng, StdRng};
use rand::seq::IndexedMutRandom;
use rand::{SeedableRng, TryRngCore};

use base64::{engine::general_purpose, Engine as _};
use futures::{stream, StreamExt};
use indexmap::IndexSet;
use rodio::{Decoder, OutputStream, Sink, Source};
use serde::Deserialize;
use tauri::{Emitter, State};

#[derive(Debug, Deserialize, Clone)]
struct VideoInfo {
    id: String,
    title: String,
}

struct AudioState {
    _stream: OutputStream,
    sink: Sink,
    playlist: Arc<Mutex<Vec<VideoInfo>>>,
    visited: IndexSet<String>,
    current: String,
    duration: Duration,
}

#[derive(Debug)]
pub enum PlayerCommand {
    Play,
    Pause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    Title(tokio::sync::oneshot::Sender<(String, bool, String, u64)>),
    Position(tokio::sync::oneshot::Sender<u64>),
    SetPosition(Duration),
}

pub async fn player_loop(
    mut cmd_rx: Receiver<PlayerCommand>,
    app_handle: tauri::AppHandle,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let playlist_url =
        "https://youtube.com/playlist?list="; //here goes the youtube playlist url you want to use
    let all_videos = get_playlist(playlist_url)?;

    let playlist = Arc::new(Mutex::new(vec![]));

    let (check, missing_ids) = check_if_full(&all_videos);

    if check {
        let mut pl = playlist.lock().unwrap();
        *pl = all_videos;
    } else {
        let mut pl = playlist.lock().unwrap();
        *pl = all_videos
            .clone()
            .into_iter()
            .filter(|v| !missing_ids.contains(&v.id))
            .collect();

        let missing_ids_cloned = missing_ids.clone();
        let all_videos_cloned = all_videos.clone();
        let playlist_clone = Arc::clone(&playlist);

        tokio::spawn(async move {
            let _ = start_downloads(all_videos_cloned, missing_ids_cloned, playlist_clone).await;
        });
    }

    let visited = IndexSet::new();

    let (sink, _stream) = create_sink();

    let current = String::from("");
    let duration = Duration::from_micros(0);
    let mut a = AudioState {
        sink,
        _stream,
        playlist,
        visited,
        current,
        duration,
    };

    // Wait for playlist to fill
    while a.playlist.lock().unwrap().is_empty() {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    a.sink.pause();
    // Load first song to start
    load_next_song(
        &a.playlist,
        &mut a.visited,
        &a.sink,
        &mut a.current,
        &mut a.duration,
    );

    // Listen for commands in a loop
    let mut first = true;
    while let Some(cmd) = cmd_rx.recv().await {
        if a.sink.empty() {
            if first {
                tokio::time::sleep(Duration::from_secs(3)).await;
                first = !first;
            }
            let (sink, _stream) = create_sink();
            a._stream = _stream;
            a.sink = sink;
            load_next_song(
                &a.playlist,
                &mut a.visited,
                &a.sink,
                &mut a.current,
                &mut a.duration,
            );
            a.sink.play();

            app_handle.emit("next_song", "")?;
        }
        match cmd {
            PlayerCommand::Play => play_song(&a.sink),
            PlayerCommand::Pause => pause_song(&a.sink),
            PlayerCommand::Next => next_song(
                &a.playlist,
                &mut a.visited,
                &a.sink,
                &mut a.current,
                &mut a.duration,
            ),
            PlayerCommand::Previous => {
                previous_song(&mut a.visited, &a.sink, &mut a.current, &mut a.duration)
            }
            PlayerCommand::Title(reply) => {
                let status = get_status(&a.playlist, &a.sink, &a.current, &a.duration);
                let _ = reply.send(status);
            }
            PlayerCommand::Position(reply) => {
                let status = poll_duration(&a.sink);
                let _ = reply.send(status);
            }
            PlayerCommand::SetPosition(pos) => {
                set_time(&a.sink, pos);
            }
            PlayerCommand::VolumeDown => {
                volume_down_cmd(&a.sink);
            }
            PlayerCommand::VolumeUp => {
                volume_up_cmd(&a.sink);
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub async fn volume_up(cmd_tx: State<'_, Sender<PlayerCommand>>) -> Result<(), String> {
    cmd_tx
        .send(PlayerCommand::VolumeUp)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn volume_down(cmd_tx: State<'_, Sender<PlayerCommand>>) -> Result<(), String> {
    cmd_tx
        .send(PlayerCommand::VolumeDown)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn play(cmd_tx: State<'_, Sender<PlayerCommand>>) -> Result<(), String> {
    cmd_tx
        .send(PlayerCommand::Play)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn pause(cmd_tx: State<'_, Sender<PlayerCommand>>) -> Result<(), String> {
    cmd_tx
        .send(PlayerCommand::Pause)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn next(cmd_tx: State<'_, Sender<PlayerCommand>>) -> Result<(), String> {
    cmd_tx
        .send(PlayerCommand::Next)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn previous(cmd_tx: State<'_, Sender<PlayerCommand>>) -> Result<(), String> {
    cmd_tx
        .send(PlayerCommand::Previous)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_title(
    cmd_tx: State<'_, Sender<PlayerCommand>>,
) -> Result<(String, bool, String, u64), String> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

    cmd_tx
        .send(PlayerCommand::Title(reply_tx))
        .await
        .map_err(|e| e.to_string())?;

    reply_rx.await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_pos(cmd_tx: State<'_, Sender<PlayerCommand>>) -> Result<u64, String> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    cmd_tx
        .send(PlayerCommand::Position(reply_tx))
        .await
        .map_err(|e| e.to_string())?;
    reply_rx.await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_thumbnail_base64(id: String) -> Result<String, String> {
    let out_dir = get_music_dir().expect("No pude determinar el directorio de música");
    let path = out_dir.join("thumbnails").join(format!("{}.jpg", id));

    // Read the file
    let data = fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;

    // Encode as base64
    let encoded = general_purpose::STANDARD.encode(&data);

    // Return a data URI
    let data_uri = format!("data:image/jpeg;base64,{}", encoded);

    Ok(data_uri)
}

#[tauri::command]
pub async fn set_position_command(
    pos_ms: u64,
    cmd_tx: tauri::State<'_, Sender<PlayerCommand>>,
) -> Result<(), String> {
    cmd_tx
        .send(PlayerCommand::SetPosition(Duration::from_millis(pos_ms)))
        .await
        .map_err(|e| e.to_string())
}

fn volume_down_cmd(sink: &Sink) {
    let current = sink.volume();
    let val: f32;
    if current < 0.02 {
        val = 0.0;
    } else {
        val = current - 0.02;
    }
    sink.set_volume(val);
}

fn volume_up_cmd(sink: &Sink) {
    let current = sink.volume();
    let val: f32;
    if current > 0.98 {
        val = 1.0;
    } else {
        val = current + 0.02;
    }
    sink.set_volume(val);
}

fn set_time(sink: &Sink, pos: Duration) {
    let _ = sink.try_seek(pos);
}

fn poll_duration(sink: &Sink) -> u64 {
    sink.get_pos().as_secs()
}

fn get_status(
    playlist: &Arc<Mutex<Vec<VideoInfo>>>,
    sink: &Sink,
    current: &String,
    duration: &Duration,
) -> (String, bool, String, u64) {
    let playlist = playlist.lock().unwrap();
    // Buscar título de la canción actual
    let title = playlist
        .iter()
        .find(|v| v.id == *current)
        .map(|v| v.title.clone())
        .unwrap_or_else(|| "Desconocido".to_string());

    // Estado del sink
    let paused = sink.is_paused();
    let id = current.clone();
    let duration = duration.clone().as_secs();
    (title, paused, id, duration)
}

fn get_playlist(playlist_url: &str) -> Result<Vec<VideoInfo>, Box<dyn Error + Send + Sync>> {
    let mut cmd = Command::new("yt-dlp");
    cmd.args(&["--flat-playlist", "--dump-json", playlist_url]);
    cmd.stdout(Stdio::piped());

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let reader = BufReader::new(stdout);

    let mut videos = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let video_info: VideoInfo = serde_json::from_str(&line)?;
        videos.push(video_info);
    }

    let status = child.wait()?;
    if !status.success() {
        return Err("yt-dlp exited with error".into());
    }

    Ok(videos)
}

fn get_music_dir() -> Option<PathBuf> {
    let home_dir = dirs::home_dir()?;
    Some(home_dir.join("Music").join("Yt Player"))
}

fn read_existing_ids(path: &Path) -> std::io::Result<HashSet<String>> {
    if !path.exists() {
        File::create(&path)?;
        return Ok(HashSet::new());
    }

    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    let mut ids = HashSet::new();

    for line in reader.lines() {
        let id = line?;
        ids.insert(id);
    }

    Ok(ids)
}

async fn download_audio_single(
    id: String,
    out_dir: PathBuf,
    ids_file: PathBuf,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let id_clone = id.clone();

    spawn_blocking(move || -> Result<(), Box<dyn Error + Send + Sync>> {
        let out_template = out_dir.join("%(id)s.%(ext)s").to_string_lossy().to_string();

        let args = vec![
            "--extract-audio".to_string(),
            "--audio-format".to_string(),
            "mp3".to_string(),
            "-o".to_string(),
            out_template,
            format!("https://www.youtube.com/watch?v={}", id),
        ];

        println!("Calling yt-dlp with args: {:?}", args);

        let status = std::process::Command::new("yt-dlp").args(&args).status()?;

        if !status.success() {
            return Err("yt-dlp exited with error".to_string().into());
        }

        // Append the ID to the file immediately after successful download
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ids_file)?;
        writeln!(file, "{}", id)?;

        Ok(())
    })
    .await
    .map_err(|e| -> Box<dyn Error + Send + Sync> { Box::new(e) })??;

    Ok(id_clone)
}

async fn download_audios_limited(
    playlist: Arc<Mutex<Vec<VideoInfo>>>,
    all_videos: Vec<VideoInfo>,
    ids: Vec<String>,
    out_dir: &Path,
    ids_file: &Path,
    limit: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let out_dir = out_dir.to_path_buf();
    let ids_file = ids_file.to_path_buf();

    // stream of futures that return ID
    let stream = stream::iter(ids.iter().cloned().map(move |id| {
        let out_dir = out_dir.clone();
        let ids_file = ids_file.clone();
        async move { download_audio_single(id, out_dir, ids_file).await }
    }));

    let mut stream = stream.buffered(limit);

    while let Some(res) = stream.next().await {
        match res {
            Ok(id) => {
                if let Some(video) = all_videos.iter().find(|v| v.id == id) {
                    let mut pl = playlist.lock().unwrap();
                    pl.push(video.clone());
                    println!("✅ Added to playlist: {}", video.title);
                }
            }
            Err(e) => {
                eprintln!("❌ Audio download failed: {e}");
            }
        }
    }

    Ok(())
}

async fn download_thumbnail(id: &str, out_dir: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    let id_owned = id.to_string();
    let out_dir = out_dir.to_path_buf();

    let id_for_closure = id_owned.clone();

    let video_json: serde_json::Value = tokio::task::spawn_blocking(
        move || -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
            let url = format!("https://www.youtube.com/watch?v={}", id_for_closure);
            let output = std::process::Command::new("yt-dlp")
                .args(&["--skip-download", "--print-json", &url])
                .output()?;

            if !output.status.success() {
                return Err("yt-dlp failed to get info".into());
            }

            let json_str = String::from_utf8(output.stdout)?;
            let video_json: serde_json::Value = serde_json::from_str(&json_str)?;
            Ok(video_json)
        },
    )
    .await??;

    // Try to get best available thumbnail URL
    let thumb_url = video_json
        .get("thumbnails")
        .and_then(|t| t.as_array())
        .and_then(|arr| {
            arr.last()
                .and_then(|t| t.get("url"))
                .and_then(|u| u.as_str())
        })
        .or_else(|| video_json.get("thumbnail").and_then(|u| u.as_str()))
        .ok_or("No valid thumbnail URL")?;

    let response = reqwest::get(thumb_url).await?;
    let bytes = response.bytes().await?;

    let thumb_dir = out_dir.join("thumbnails");
    tokio::fs::create_dir_all(&thumb_dir).await?;

    let thumb_path = thumb_dir.join(format!("{}.jpg", id_owned));
    let mut file = tokio::fs::File::create(thumb_path).await?;
    file.write_all(&bytes).await?;

    println!("Thumbnail downloaded for {}", id_owned);
    Ok(())
}

async fn download_thumbnails_limited(
    ids: Vec<String>,
    out_dir: &Path,
    limit: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let out_dir = out_dir.to_path_buf(); // Hace el Path 'static'

    let stream = stream::iter(ids.into_iter().map(move |id| {
        let out_dir = out_dir.clone();
        async move { download_thumbnail(&id, &out_dir).await }
    }));

    let results = stream.buffered(limit).collect::<Vec<_>>().await;
    for res in results {
        res?;
    }
    Ok(())
}

fn check_if_full(all_videos: &Vec<VideoInfo>) -> (bool, Vec<std::string::String>) {
    let out_dir = get_music_dir().expect("No pude determinar el directorio de música");
    fs::create_dir_all(&out_dir).unwrap();

    let ids_file = out_dir.join("downloaded_ids.txt");

    let existing_ids = read_existing_ids(&ids_file).unwrap();

    let missing_ids: Vec<_> = all_videos
        .iter()
        .filter(|video| !existing_ids.contains(&video.id))
        .map(|video| video.id.clone())
        .collect();

    if missing_ids.is_empty() {
        println!("No hay nuevas canciones para descargar!");
        return (true, missing_ids);
    }

    return (false, missing_ids);
}

// Función para ejecutar descargas con límite en paralelo para audios y thumbnails
async fn start_downloads(
    all_videos: Vec<VideoInfo>,
    missing_ids: Vec<String>,
    playlist: Arc<Mutex<Vec<VideoInfo>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let out_dir = get_music_dir().expect("No pude determinar el directorio de música");
    fs::create_dir_all(&out_dir)?;

    let ids_file = out_dir.join("downloaded_ids.txt");

    // Descarga audios en chunks de tamaño 2
    let audio_task = download_audios_limited(
        playlist,
        all_videos,
        missing_ids.clone(),
        &out_dir,
        &ids_file,
        2,
    );

    // Descarga thumbnails limitando a 2 concurrencias
    let thumb_task = download_thumbnails_limited(missing_ids, &out_dir, 2);

    let (audio_res, thumb_res) = tokio::join!(audio_task, thumb_task);

    audio_res?;
    thumb_res?;

    Ok(())
}

fn create_sink() -> (Sink, OutputStream) {
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    // Load a sound from a file, using a path relative to Cargo.toml
    let sink = Sink::try_new(&stream_handle).unwrap();
    (sink, _stream)
}

fn select_next_song(videos: &Vec<VideoInfo>, visitados: &mut IndexSet<String>) -> Option<String> {
    let mut available: Vec<_> = videos
        .iter()
        .filter(|item| !visitados.contains(&item.id))
        .collect();

    let seed = OsRng.try_next_u64().unwrap();
    let mut rng = StdRng::seed_from_u64(seed);

    if let Some(choice) = available.choose_mut(&mut rng) {
        // Marcar como visitado
        visitados.insert(choice.id.clone());
        Some(choice.id.clone())
    } else {
        // Si ya no quedan, limpiar y volver a empezar
        visitados.clear();

        if let Some(choice) = videos.choose(&mut rng) {
            visitados.insert(choice.id.clone());
            Some(choice.id.clone())
        } else {
            None
        }
    }
}

fn load_previus_song(
    visitados: &mut IndexSet<String>,
    sink: &Sink,
    current: &mut String,
    duration: &mut Duration,
) -> bool {
    if visitados.len() == 1 {
        return false;
    }
    let id_song = visitados
        .get_index(visitados.len() - 2)
        .expect("no se logro obtener el ultimo elemento");
    *current = id_song.clone();
    let id = id_song.to_owned() + ".mp3";

    visitados.shift_remove_index(visitados.len() - 1);

    let path = get_music_dir()
        .expect("hubo un problema obteniendo el path")
        .join(&id);

    let file = BufReader::new(File::open(&path).unwrap());
    // Decode that sound file into a source
    let source = Decoder::new(file).unwrap();

    *duration = source.total_duration().unwrap();

    sink.append(source);
    true
}

fn load_next_song(
    videos: &Arc<Mutex<Vec<VideoInfo>>>,
    visitados: &mut IndexSet<String>,
    sink: &Sink,
    current: &mut String,
    duration: &mut Duration,
) -> bool {
    let videos_guard = videos.lock().unwrap();
    let mut id = select_next_song(&videos_guard, visitados).expect("Error eligiendo canción");

    *current = id.clone();
    id = id + ".mp3";
    let path = get_music_dir()
        .expect("hubo un problema obteniendo el path")
        .join(&id);
    let file = BufReader::new(File::open(&path).unwrap());
    // Decode that sound file into a source
    let source = Decoder::new(file).unwrap();

    *duration = source.total_duration().unwrap();

    sink.append(source);
    true
}

fn next_song(
    videos: &Arc<Mutex<Vec<VideoInfo>>>,
    visitados: &mut IndexSet<String>,
    sink: &Sink,
    current: &mut String,
    duration: &mut Duration,
) {
    load_next_song(videos, visitados, sink, current, duration);
    sink.skip_one();
}

fn previous_song(
    visitados: &mut IndexSet<String>,
    sink: &Sink,
    current: &mut String,
    duration: &mut Duration,
) {
    if load_previus_song(visitados, sink, current, duration) {
        sink.skip_one();
    }
}

fn play_song(sink: &Sink) {
    sink.play();
}

fn pause_song(sink: &Sink) {
    sink.pause();
}
