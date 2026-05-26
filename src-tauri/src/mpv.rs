use crate::domain::{MpvLaunchRequest, MpvLaunchResult};
use crate::error::{AppError, AppResult};
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicI64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static NEXT_IPC_REQUEST_ID: AtomicI64 = AtomicI64::new(1);
const IPC_RETRY_COUNT: usize = 60;
const IPC_RETRY_DELAY: Duration = Duration::from_millis(100);
const IPC_CONNECT_RETRY_COUNT: usize = 40;
const IPC_CONNECT_RETRY_DELAY: Duration = Duration::from_millis(50);

#[derive(Default)]
pub struct MpvController;

pub fn launch(
    _controller: &tauri::State<'_, MpvController>,
    request: MpvLaunchRequest,
) -> AppResult<MpvLaunchResult> {
    validate_launch_request(&request)?;

    let ipc_pipe_path = next_ipc_pipe_path();
    let mut command = Command::new(&request.mpv_path);
    append_launch_args(&mut command, &request, &ipc_pipe_path);

    let child = command
        .spawn()
        .map_err(|error| AppError::MpvLaunch(error.to_string()))?;
    let process_id = child.id();

    let tracks = wait_for_track_list_ready(&ipc_pipe_path, &request)?;
    let subtitle_tracks = parse_subtitle_tracks(&tracks);
    dump_subtitle_tracks(&subtitle_tracks);

    let (primary_track_id, secondary_track_id) =
        resolve_requested_subtitle_track_ids(&request, &subtitle_tracks)?;

    apply_subtitle_track_selection(&ipc_pipe_path, primary_track_id, secondary_track_id)?;
    verify_subtitle_track_selection(&ipc_pipe_path, primary_track_id, secondary_track_id)?;

    Ok(MpvLaunchResult {
        process_id,
        argument_count: mpv_argument_count(&request),
        reused_existing: false,
        switched_video: false,
    })
}

pub fn reveal(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::MissingFile(path.to_path_buf()));
    }

    Command::new("explorer")
        .arg(path)
        .spawn()
        .map_err(|error| AppError::MpvLaunch(error.to_string()))?;
    Ok(())
}

fn validate_launch_request(request: &MpvLaunchRequest) -> AppResult<()> {
    if !request.video_path.is_file() {
        return Err(AppError::MissingFile(request.video_path.to_path_buf()));
    }
    if let Some(primary) = &request.primary_subtitle {
        ensure_subtitle_exists(primary)?;
    }
    if let Some(secondary) = &request.secondary_subtitle {
        ensure_subtitle_exists(secondary)?;
    }
    Ok(())
}

fn ensure_subtitle_exists(path: &Path) -> AppResult<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(AppError::MissingFile(path.to_path_buf()))
    }
}

fn append_launch_args(command: &mut Command, request: &MpvLaunchRequest, ipc_pipe_path: &str) {
    command.arg(&request.video_path);
    command.arg("--idle=yes");
    command.arg("--sub-auto=no");
    command.arg(format!("--input-ipc-server={ipc_pipe_path}"));
    append_subtitle_args(command, request);
    append_extra_args(command, request);
}

fn append_subtitle_args(command: &mut Command, request: &MpvLaunchRequest) {
    if let Some(primary) = &request.primary_subtitle {
        command.arg(format!("--sub-file={}", primary.display()));
    }
    if let Some(secondary) = &request.secondary_subtitle {
        command.arg(format!("--sub-file={}", secondary.display()));
    }
}

fn append_extra_args(command: &mut Command, request: &MpvLaunchRequest) {
    for arg in &request.extra_args {
        if !arg.trim().is_empty() {
            command.arg(arg);
        }
    }
}

#[derive(Debug, Clone)]
struct MpvSubtitleTrack {
    id: i64,
    title: Option<String>,
    lang: Option<String>,
    external: bool,
    external_filename: Option<String>,
    filename: Option<String>,
    selected: bool,
    main_selection: Option<i64>,
    codec: Option<String>,
}

fn wait_for_track_list_ready(pipe_path: &str, request: &MpvLaunchRequest) -> AppResult<Value> {
    let mut last_tracks = Value::Null;
    for _ in 0..IPC_RETRY_COUNT {
        let tracks = send_ipc_request(pipe_path, json!(["get_property", "track-list"]))?;
        if track_list_ready_for_request(&tracks, request) {
            return Ok(tracks);
        }
        last_tracks = tracks;
        thread::sleep(IPC_RETRY_DELAY);
    }

    println!("========== MPV track-list timeout snapshot ==========");
    dump_track_list(&last_tracks);
    println!(
        "expected_external_subtitle_count = {}",
        expected_external_subtitle_count(request)
    );
    println!(
        "actual_external_subtitle_count = {}",
        external_subtitle_count(&last_tracks)
    );

    Err(AppError::MpvLaunch(
        "MPV track-list was not ready for requested external subtitles".to_owned(),
    ))
}

fn track_list_has_video(track_list: &Value) -> bool {
    track_list.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|track| track.get("type").and_then(Value::as_str) == Some("video"))
    })
}

fn expected_external_subtitle_count(request: &MpvLaunchRequest) -> usize {
    usize::from(request.primary_subtitle.is_some())
        + usize::from(request.secondary_subtitle.is_some())
}

fn external_subtitle_count(track_list: &Value) -> usize {
    track_list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|track| {
            track.get("type").and_then(Value::as_str) == Some("sub")
                && track
                    .get("external")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .count()
}

fn track_list_ready_for_request(track_list: &Value, request: &MpvLaunchRequest) -> bool {
    if !track_list_has_video(track_list) {
        return false;
    }

    let expected = expected_external_subtitle_count(request);
    if expected == 0 {
        return true;
    }

    external_subtitle_count(track_list) >= expected
}

fn parse_subtitle_tracks(track_list: &Value) -> Vec<MpvSubtitleTrack> {
    match track_list.as_array() {
        Some(items) => items
            .iter()
            .filter(|track| track.get("type").and_then(Value::as_str) == Some("sub"))
            .filter_map(parse_subtitle_track)
            .collect(),
        None => Vec::new(),
    }
}

fn parse_subtitle_track(track: &Value) -> Option<MpvSubtitleTrack> {
    Some(MpvSubtitleTrack {
        id: track.get("id")?.as_i64()?,
        title: string_property(track, "title"),
        lang: string_property(track, "lang"),
        external: track
            .get("external")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        external_filename: string_property(track, "external-filename"),
        filename: string_property(track, "filename"),
        selected: track
            .get("selected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        main_selection: track.get("main-selection").and_then(Value::as_i64),
        codec: string_property(track, "codec"),
    })
}

fn string_property(track: &Value, key: &str) -> Option<String> {
    track
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn dump_track_list(track_list: &Value) {
    println!("========== MPV track-list ==========");
    match serde_json::to_string_pretty(track_list) {
        Ok(text) => println!("{text}"),
        Err(error) => println!("Failed to format track-list JSON: {error}; raw={track_list}"),
    }
    println!("====================================");
}

fn dump_subtitle_tracks(tracks: &[MpvSubtitleTrack]) {
    println!("========== MPV subtitle tracks ==========");
    if tracks.is_empty() {
        println!("No subtitle tracks found.");
    }
    for track in tracks {
        println!(
            "id={} title={:?} lang={:?} external={} external_filename={:?} filename={:?} selected={} main_selection={:?} codec={:?}",
            track.id,
            track.title,
            track.lang,
            track.external,
            track.external_filename,
            track.filename,
            track.selected,
            track.main_selection,
            track.codec
        );
    }
    println!("=========================================");
}

fn normalize_path_text(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn normalize_path_for_match(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn find_subtitle_track_id_by_path(
    tracks: &[MpvSubtitleTrack],
    subtitle_path: &Path,
) -> Option<i64> {
    let wanted = normalize_path_for_match(subtitle_path);

    tracks.iter().find_map(|track| {
        if !track.external {
            return None;
        }

        let external_filename = track.external_filename.as_deref()?;
        let actual = normalize_path_text(external_filename);

        if actual == wanted {
            Some(track.id)
        } else {
            None
        }
    })
}

fn resolve_requested_subtitle_track_ids(
    request: &MpvLaunchRequest,
    tracks: &[MpvSubtitleTrack],
) -> AppResult<(Option<i64>, Option<i64>)> {
    let primary_track_id = match &request.primary_subtitle {
        Some(primary) => {
            let track_id = find_subtitle_track_id_by_path(tracks, primary).ok_or_else(|| {
                AppError::MpvLaunch(format!(
                    "未在 MPV track-list 中找到主字幕：{}",
                    primary.display()
                ))
            })?;
            Some(track_id)
        }
        None => None,
    };

    let secondary_track_id = match &request.secondary_subtitle {
        Some(secondary) => {
            let track_id = find_subtitle_track_id_by_path(tracks, secondary).ok_or_else(|| {
                AppError::MpvLaunch(format!(
                    "未在 MPV track-list 中找到副字幕：{}",
                    secondary.display()
                ))
            })?;
            Some(track_id)
        }
        None => None,
    };

    if primary_track_id.is_some()
        && secondary_track_id.is_some()
        && primary_track_id == secondary_track_id
    {
        return Err(AppError::MpvLaunch(
            "主字幕和副字幕不能解析到同一个 MPV track id".to_owned(),
        ));
    }

    Ok((primary_track_id, secondary_track_id))
}

fn apply_subtitle_track_selection(
    pipe_path: &str,
    primary_track_id: Option<i64>,
    secondary_track_id: Option<i64>,
) -> AppResult<()> {
    match primary_track_id {
        Some(id) => {
            send_ipc_request(pipe_path, json!(["set_property", "sid", id]))?;
            send_ipc_request(pipe_path, json!(["set_property", "sub-visibility", true]))?;
        }
        None => {
            send_ipc_request(pipe_path, json!(["set_property", "sid", "no"]))?;
        }
    }

    match secondary_track_id {
        Some(id) => {
            send_ipc_request(pipe_path, json!(["set_property", "secondary-sid", id]))?;
            send_ipc_request(
                pipe_path,
                json!(["set_property", "secondary-sub-visibility", true]),
            )?;
        }
        None => {
            send_ipc_request(pipe_path, json!(["set_property", "secondary-sid", "no"]))?;
        }
    }

    Ok(())
}

fn verify_subtitle_track_selection(
    pipe_path: &str,
    primary_track_id: Option<i64>,
    secondary_track_id: Option<i64>,
) -> AppResult<()> {
    let tracks_value = send_ipc_request(pipe_path, json!(["get_property", "track-list"]))?;
    let tracks = parse_subtitle_tracks(&tracks_value);

    dump_subtitle_tracks(&tracks);

    verify_subtitle_track_main_selection(&tracks, primary_track_id, Some(0), "主字幕")?;
    verify_subtitle_track_main_selection(&tracks, secondary_track_id, Some(1), "副字幕")?;

    Ok(())
}

fn verify_subtitle_track_main_selection(
    tracks: &[MpvSubtitleTrack],
    track_id: Option<i64>,
    expected_main_selection: Option<i64>,
    role: &str,
) -> AppResult<()> {
    let Some(track_id) = track_id else {
        return Ok(());
    };

    let selected = tracks
        .iter()
        .any(|track| track.id == track_id && track.main_selection == expected_main_selection);

    if selected {
        Ok(())
    } else {
        Err(AppError::MpvLaunch(format!(
            "{role} track id {track_id} 未成功成为 main_selection={}",
            expected_main_selection
                .map(|value| value.to_string())
                .unwrap_or_else(|| "None".to_owned())
        )))
    }
}

fn send_ipc_request(pipe_path: &str, command: Value) -> AppResult<Value> {
    let request_id = NEXT_IPC_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let payload = serde_json::to_string(&json!({
        "command": command,
        "request_id": request_id,
    }))?;
    let mut pipe = open_ipc_pipe(pipe_path)?;
    pipe.write_all(payload.as_bytes())?;
    pipe.write_all(b"\n")?;
    pipe.flush()?;

    let mut reader = BufReader::new(pipe);
    let mut response = String::new();
    loop {
        response.clear();
        let bytes = reader.read_line(&mut response)?;
        if bytes == 0 {
            return Err(AppError::MpvLaunch(
                "MPV IPC connection was closed".to_owned(),
            ));
        }

        let value = serde_json::from_str::<Value>(&response)?;
        if value.get("request_id").and_then(Value::as_i64) != Some(request_id) {
            continue;
        }

        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("success");
        if error != "success" {
            return Err(AppError::MpvLaunch(format!(
                "MPV IPC command failed: {error}"
            )));
        }
        return Ok(value.get("data").cloned().unwrap_or(Value::Null));
    }
}

fn open_ipc_pipe(pipe_path: &str) -> AppResult<std::fs::File> {
    let mut last_error = None;
    for _ in 0..IPC_CONNECT_RETRY_COUNT {
        match OpenOptions::new().read(true).write(true).open(pipe_path) {
            Ok(file) => return Ok(file),
            Err(error) => {
                last_error = Some(error);
                thread::sleep(IPC_CONNECT_RETRY_DELAY);
            }
        }
    }

    Err(AppError::MpvLaunch(
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "Could not connect to MPV IPC pipe".to_owned()),
    ))
}

fn next_ipc_pipe_path() -> String {
    format!(r"\\.\pipe\mpv-tidy-{}-{}", std::process::id(), now_unix())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn mpv_argument_count(request: &MpvLaunchRequest) -> usize {
    let subtitle_count = usize::from(request.primary_subtitle.is_some())
        + usize::from(request.secondary_subtitle.is_some());
    4 + subtitle_count
        + request
            .extra_args
            .iter()
            .filter(|arg| !arg.trim().is_empty())
            .count()
}

#[cfg(test)]
mod tests {
    use super::{
        expected_external_subtitle_count, external_subtitle_count, find_subtitle_track_id_by_path,
        mpv_argument_count, parse_subtitle_tracks, resolve_requested_subtitle_track_ids,
        track_list_has_video, track_list_ready_for_request, verify_subtitle_track_main_selection,
        MpvSubtitleTrack,
    };
    use crate::domain::MpvLaunchRequest;
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn counts_minimal_mpv_arguments() {
        let request = MpvLaunchRequest {
            mpv_path: PathBuf::from("mpv"),
            video_path: PathBuf::from("S01E01.mkv"),
            primary_subtitle: Some(PathBuf::from("S01E01.zh-Hans.ass")),
            secondary_subtitle: None,
            extra_args: vec!["--save-position-on-quit".to_owned(), " ".to_owned()],
        };

        assert_eq!(mpv_argument_count(&request), 6);
    }

    #[test]
    fn detects_video_track_list_readiness() {
        let tracks = json!([
            { "id": 1, "type": "video", "codec": "h264" },
            { "id": 1, "type": "audio", "codec": "aac" }
        ]);

        assert!(track_list_has_video(&tracks));
    }

    #[test]
    fn parses_subtitle_tracks_for_debug_output() {
        let tracks = json!([
            { "id": 1, "type": "video", "codec": "h264" },
            {
                "id": 3,
                "type": "sub",
                "title": "zh-Hans.ass",
                "lang": "zh-Hans",
                "external": true,
                "external-filename": "D:/Anime/zh-Hans.ass",
                "selected": true,
                "main-selection": 0,
                "codec": "ass"
            }
        ]);

        let subtitles = parse_subtitle_tracks(&tracks);

        assert_eq!(subtitles.len(), 1);
        assert_eq!(subtitles[0].id, 3);
        assert!(subtitles[0].external);
    }

    #[test]
    fn counts_expected_external_subtitles_from_request() {
        let request = MpvLaunchRequest {
            mpv_path: PathBuf::from("mpv"),
            video_path: PathBuf::from("S01E01.mkv"),
            primary_subtitle: Some(PathBuf::from("S01E01.zh-Hans.ass")),
            secondary_subtitle: Some(PathBuf::from("S01E01.ja.srt")),
            extra_args: Vec::new(),
        };

        assert_eq!(expected_external_subtitle_count(&request), 2);
    }

    #[test]
    fn counts_external_subtitles_in_track_list() {
        let tracks = json!([
            { "id": 1, "type": "video" },
            { "id": 1, "type": "sub", "external": false },
            { "id": 2, "type": "sub", "external": true },
            { "id": 3, "type": "sub", "external": true }
        ]);

        assert_eq!(external_subtitle_count(&tracks), 2);
    }

    #[test]
    fn track_list_is_ready_without_requested_external_subtitles_once_video_exists() {
        let request = MpvLaunchRequest {
            mpv_path: PathBuf::from("mpv"),
            video_path: PathBuf::from("S01E01.mkv"),
            primary_subtitle: None,
            secondary_subtitle: None,
            extra_args: Vec::new(),
        };
        let tracks = json!([{ "id": 1, "type": "video" }]);

        assert!(track_list_ready_for_request(&tracks, &request));
    }

    #[test]
    fn track_list_waits_for_requested_external_subtitles() {
        let request = MpvLaunchRequest {
            mpv_path: PathBuf::from("mpv"),
            video_path: PathBuf::from("S01E01.mkv"),
            primary_subtitle: Some(PathBuf::from("S01E01.zh-Hans.ass")),
            secondary_subtitle: Some(PathBuf::from("S01E01.ja.srt")),
            extra_args: Vec::new(),
        };
        let one_external_subtitle = json!([
            { "id": 1, "type": "video" },
            { "id": 2, "type": "sub", "external": true }
        ]);
        let two_external_subtitles = json!([
            { "id": 1, "type": "video" },
            { "id": 2, "type": "sub", "external": true },
            { "id": 3, "type": "sub", "external": true }
        ]);

        assert!(!track_list_ready_for_request(
            &one_external_subtitle,
            &request
        ));
        assert!(track_list_ready_for_request(
            &two_external_subtitles,
            &request
        ));
    }

    #[test]
    fn finds_external_subtitle_track_id_by_full_path() {
        let tracks = vec![
            subtitle_track(1, false, None, None),
            subtitle_track(2, true, Some("D:\\Anime\\Subs\\ja.srt"), Some(1)),
            subtitle_track(3, true, Some("D:/Anime/Subs/zh-Hans.ass"), Some(0)),
        ];

        assert_eq!(
            find_subtitle_track_id_by_path(&tracks, &PathBuf::from("D:\\Anime\\Subs\\zh-Hans.ass")),
            Some(3)
        );
    }

    #[test]
    fn resolves_requested_primary_and_secondary_track_ids() {
        let request = MpvLaunchRequest {
            mpv_path: PathBuf::from("mpv"),
            video_path: PathBuf::from("S01E01.mkv"),
            primary_subtitle: Some(PathBuf::from("D:\\Anime\\Subs\\zh-Hans.ass")),
            secondary_subtitle: Some(PathBuf::from("D:\\Anime\\Subs\\ja.srt")),
            extra_args: Vec::new(),
        };
        let tracks = vec![
            subtitle_track(1, false, None, None),
            subtitle_track(2, true, Some("D:/Anime/Subs/ja.srt"), None),
            subtitle_track(3, true, Some("D:/Anime/Subs/zh-Hans.ass"), None),
        ];

        let resolved = resolve_requested_subtitle_track_ids(&request, &tracks);

        assert!(matches!(resolved, Ok((Some(3), Some(2)))));
    }

    #[test]
    fn rejects_missing_requested_primary_subtitle_track() {
        let request = MpvLaunchRequest {
            mpv_path: PathBuf::from("mpv"),
            video_path: PathBuf::from("S01E01.mkv"),
            primary_subtitle: Some(PathBuf::from("D:\\Anime\\Subs\\zh-Hans.ass")),
            secondary_subtitle: None,
            extra_args: Vec::new(),
        };
        let tracks = vec![subtitle_track(2, true, Some("D:/Anime/Subs/ja.srt"), None)];

        let resolved = resolve_requested_subtitle_track_ids(&request, &tracks);

        assert!(resolved
            .err()
            .map(|error| error.to_string().contains("主字幕"))
            .unwrap_or(false));
    }

    #[test]
    fn rejects_primary_and_secondary_resolving_to_same_track_id() {
        let request = MpvLaunchRequest {
            mpv_path: PathBuf::from("mpv"),
            video_path: PathBuf::from("S01E01.mkv"),
            primary_subtitle: Some(PathBuf::from("D:\\Anime\\Subs\\zh-Hans.ass")),
            secondary_subtitle: Some(PathBuf::from("D:\\Anime\\Subs\\zh-Hans.ass")),
            extra_args: Vec::new(),
        };
        let tracks = vec![subtitle_track(
            3,
            true,
            Some("D:/Anime/Subs/zh-Hans.ass"),
            None,
        )];

        let resolved = resolve_requested_subtitle_track_ids(&request, &tracks);

        assert!(resolved
            .err()
            .map(|error| error.to_string().contains("同一个 MPV track id"))
            .unwrap_or(false));
    }

    #[test]
    fn verifies_subtitle_main_selection() {
        let tracks = vec![
            subtitle_track(2, true, Some("D:/Anime/Subs/ja.srt"), Some(1)),
            subtitle_track(3, true, Some("D:/Anime/Subs/zh-Hans.ass"), Some(0)),
        ];

        assert!(verify_subtitle_track_main_selection(&tracks, Some(3), Some(0), "主字幕").is_ok());
        assert!(verify_subtitle_track_main_selection(&tracks, Some(2), Some(1), "副字幕").is_ok());
        assert!(verify_subtitle_track_main_selection(&tracks, Some(3), Some(1), "副字幕").is_err());
    }

    fn subtitle_track(
        id: i64,
        external: bool,
        external_filename: Option<&str>,
        main_selection: Option<i64>,
    ) -> MpvSubtitleTrack {
        MpvSubtitleTrack {
            id,
            title: None,
            lang: None,
            external,
            external_filename: external_filename.map(ToOwned::to_owned),
            filename: None,
            selected: main_selection.is_some(),
            main_selection,
            codec: None,
        }
    }
}
