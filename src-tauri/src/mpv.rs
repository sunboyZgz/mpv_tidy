use crate::domain::{MpvLaunchRequest, MpvLaunchResult};
use crate::error::{AppError, AppResult};
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static NEXT_IPC_REQUEST_ID: AtomicI64 = AtomicI64::new(1);
const IPC_RETRY_COUNT: usize = 60;
const IPC_RETRY_DELAY: Duration = Duration::from_millis(100);
const IPC_CONNECT_RETRY_COUNT: usize = 40;
const IPC_CONNECT_RETRY_DELAY: Duration = Duration::from_millis(50);

#[derive(Default)]
pub struct MpvController {
    session: Mutex<Option<MpvSession>>,
}

struct MpvSession {
    child: Child,
    process_id: u32,
    ipc_pipe_path: String,
    current_video_path: PathBuf,
}

pub fn launch(
    controller: &tauri::State<'_, MpvController>,
    request: MpvLaunchRequest,
) -> AppResult<MpvLaunchResult> {
    validate_launch_request(&request)?;
    let mut session = controller
        .session
        .lock()
        .map_err(|_| AppError::MpvLaunch("MPV 状态锁定失败".to_owned()))?;

    if let Some(existing) = session.as_mut() {
        if mpv_process_is_alive(existing)? {
            println!(
                "===========Found existing MPV with video path {:?}, request video path {:?}===========",
                existing.current_video_path, request.video_path
            );
            if same_path(&existing.current_video_path, &request.video_path) {
                bring_process_to_front(existing.process_id);
                return Ok(MpvLaunchResult {
                    process_id: existing.process_id,
                    argument_count: 0,
                    reused_existing: true,
                    switched_video: false,
                });
            }

            switch_existing_mpv_session(existing, &request)?;
            existing.current_video_path = request.video_path.to_path_buf();
            bring_process_to_front(existing.process_id);
            return Ok(MpvLaunchResult {
                process_id: existing.process_id,
                argument_count: 0,
                reused_existing: true,
                switched_video: true,
            });
        }

        *session = None;
    }

    let next_session = spawn_mpv_session(&request)?;
    let result = MpvLaunchResult {
        process_id: next_session.process_id,
        argument_count: mpv_argument_count(&request),
        reused_existing: false,
        switched_video: false,
    };
    *session = Some(next_session);
    Ok(result)
}

fn spawn_mpv_session(request: &MpvLaunchRequest) -> AppResult<MpvSession> {
    let ipc_pipe_path = next_ipc_pipe_path();
    let mut command = Command::new(&request.mpv_path);
    append_launch_args(&mut command, request, &ipc_pipe_path);

    let child = command
        .spawn()
        .map_err(|error| AppError::MpvLaunch(error.to_string()))?;
    let process_id = child.id();

    let tracks = wait_for_track_list_ready(&ipc_pipe_path, request)?;
    let subtitle_tracks = parse_subtitle_tracks(&tracks);
    dump_subtitle_tracks(&subtitle_tracks);

    let (primary_track_id, secondary_track_id) =
        resolve_requested_subtitle_track_ids(request, &subtitle_tracks)?;

    apply_subtitle_track_selection(&ipc_pipe_path, primary_track_id, secondary_track_id)?;
    verify_subtitle_track_selection(&ipc_pipe_path, primary_track_id, secondary_track_id)?;

    Ok(MpvSession {
        child,
        process_id,
        ipc_pipe_path,
        current_video_path: request.video_path.to_path_buf(),
    })
}

fn switch_existing_mpv_session(session: &MpvSession, request: &MpvLaunchRequest) -> AppResult<()> {
    println!(
        "==========switch_existing_mpv_session: request video path {:?}===========",
        request.video_path
    );

    println!("step 1: remove external subtitle tracks");
    remove_external_subtitle_tracks(&session.ipc_pipe_path)?;
    println!("step 1 done");

    println!("step 2: clear sub-files list");
    clear_sub_files_list(&session.ipc_pipe_path)?;
    println!("step 2 done");

    println!("step 3: loadfile replace");
    send_ipc_request(
        &session.ipc_pipe_path,
        json!(["loadfile", path_for_mpv(&request.video_path), "replace"]),
    )?;
    println!("step 3 done");

    println!("step 4: wait for requested video path");
    wait_for_video_track_list_ready(&session.ipc_pipe_path, &request.video_path)?;
    println!("step 4 done");

    println!("step 5: apply requested subtitles");
    apply_requested_subtitles_to_running_mpv(&session.ipc_pipe_path, request)?;
    println!("step 5 done");

    Ok(())
}

fn apply_requested_subtitles_to_running_mpv(
    pipe_path: &str,
    request: &MpvLaunchRequest,
) -> AppResult<()> {
    add_requested_subtitles_by_ipc(pipe_path, request)?;

    let tracks = wait_for_track_list_ready(pipe_path, request)?;
    let subtitle_tracks = parse_subtitle_tracks(&tracks);

    dump_subtitle_tracks(&subtitle_tracks);

    let (primary_track_id, secondary_track_id) =
        resolve_requested_subtitle_track_ids(request, &subtitle_tracks)?;

    apply_subtitle_track_selection(pipe_path, primary_track_id, secondary_track_id)?;

    verify_subtitle_track_selection(pipe_path, primary_track_id, secondary_track_id)?;

    Ok(())
}

fn add_requested_subtitles_by_ipc(pipe_path: &str, request: &MpvLaunchRequest) -> AppResult<()> {
    println!("========== MPV sub-add request ==========");
    println!(
        "primary_subtitle = {:?}",
        request
            .primary_subtitle
            .as_ref()
            .map(|path| path.display().to_string())
    );
    println!(
        "secondary_subtitle = {:?}",
        request
            .secondary_subtitle
            .as_ref()
            .map(|path| path.display().to_string())
    );

    if let Some(primary) = &request.primary_subtitle {
        let subtitle_path = path_for_mpv(primary);
        let title = subtitle_title(primary, "primary");

        println!("sub-add primary path = {subtitle_path}");
        println!("sub-add primary title = {title}");

        send_ipc_request(pipe_path, json!(["sub-add", subtitle_path, "auto", title]))?;
    }

    if let Some(secondary) = &request.secondary_subtitle {
        let subtitle_path = path_for_mpv(secondary);
        let title = subtitle_title(secondary, "secondary");

        println!("sub-add secondary path = {subtitle_path}");
        println!("sub-add secondary title = {title}");

        send_ipc_request(pipe_path, json!(["sub-add", subtitle_path, "auto", title]))?;
    }

    println!("=========================================");

    Ok(())
}

fn subtitle_title(path: &Path, fallback: &str) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| fallback.to_owned())
}

fn remove_external_subtitle_tracks(pipe_path: &str) -> AppResult<()> {
    let tracks_value = send_ipc_request(pipe_path, json!(["get_property", "track-list"]))?;
    let subtitle_tracks = parse_subtitle_tracks(&tracks_value);

    let mut external_subtitle_ids: Vec<i64> = subtitle_tracks
        .iter()
        .filter(|track| track.external)
        .map(|track| track.id)
        .collect();

    if external_subtitle_ids.is_empty() {
        println!("No external subtitle tracks to remove.");
        return Ok(());
    }

    // 倒序删除，方便调试观察。
    external_subtitle_ids.sort_by(|a, b| b.cmp(a));

    // println!("=======Removing external subtitle tracks from existing MPV session========");
    for track_id in external_subtitle_ids {
        // println!(
        //     "remove external subtitle track id = {track_id}, track filename = {:?}",
        //     subtitle_tracks
        //         .iter()
        //         .find(|track| track.id == track_id)
        //         .and_then(|track| track.external_filename.as_deref())
        // );
        send_ipc_request(pipe_path, json!(["sub-remove", track_id]))?;
    }
    // println!("===============");

    wait_until_no_external_subtitle_tracks(pipe_path)?;
    Ok(())
}

fn wait_until_no_external_subtitle_tracks(pipe_path: &str) -> AppResult<()> {
    let mut last_tracks = Value::Null;

    for _ in 0..IPC_RETRY_COUNT {
        let tracks_value = send_ipc_request(pipe_path, json!(["get_property", "track-list"]))?;
        let subtitle_tracks = parse_subtitle_tracks(&tracks_value);

        let has_external = subtitle_tracks.iter().any(|track| track.external);

        if !has_external {
            println!("All external subtitle tracks removed.");
            return Ok(());
        }

        last_tracks = tracks_value;
        thread::sleep(IPC_RETRY_DELAY);
    }

    println!("========== external subtitles still remain ==========");
    dump_track_list(&last_tracks);

    Err(AppError::MpvLaunch(
        "MPV external subtitle tracks were not removed in time".to_owned(),
    ))
}

fn clear_sub_files_list(pipe_path: &str) -> AppResult<()> {
    send_ipc_request(pipe_path, json!(["change-list", "sub-files", "clr", ""]))?;
    Ok(())
}

fn mpv_process_is_alive(session: &mut MpvSession) -> AppResult<bool> {
    session
        .child
        .try_wait()
        .map(|status| status.is_none())
        .map_err(Into::into)
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

fn wait_for_video_track_list_ready(pipe_path: &str, video_path: &Path) -> AppResult<()> {
    let wanted = normalize_path_for_match(video_path);
    let mut last_path = Value::Null;
    // 等待 MPV 切换到目标视频。通常 MPV 会先把 path 属性切换到新视频的路径，然后才更新 track-list 以反映新视频的轨道信息。因此在 track-list 准备好之前，path 属性应该已经切换到目标视频了。
    // 不然会出现bug， thread::sleep(Duration::from_millis(300))必须使用。
    thread::sleep(Duration::from_millis(300));
    println!("wait video wanted path = {wanted}");

    for attempt in 0..IPC_RETRY_COUNT {
        println!("wait video attempt {attempt}: before get_property path");

        let current_path = send_ipc_request(pipe_path, json!(["get_property", "path"]))?;

        println!("wait video attempt {attempt}: raw path = {current_path}");

        last_path = current_path.clone();

        let actual = current_path
            .as_str()
            .map(normalize_path_text)
            .unwrap_or_else(|| "<none>".to_owned());

        println!("wait video attempt {attempt}: normalized actual path = {actual}");

        if actual == wanted {
            println!("MPV loaded requested video: {}", video_path.display());
            return Ok(());
        }

        thread::sleep(IPC_RETRY_DELAY);
    }

    Err(AppError::MpvLaunch(format!(
        "MPV 未能及时切换到目标视频。wanted={wanted}, last_path={last_path}"
    )))
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
    track_list_has_video(track_list) && track_list_contains_requested_subtitles(track_list, request)
}

fn track_list_contains_requested_subtitles(track_list: &Value, request: &MpvLaunchRequest) -> bool {
    let tracks = parse_subtitle_tracks(track_list);

    let primary_ok = match &request.primary_subtitle {
        Some(primary) => find_subtitle_track_id_by_path(&tracks, primary).is_some(),
        None => true,
    };

    let secondary_ok = match &request.secondary_subtitle {
        Some(secondary) => find_subtitle_track_id_by_path(&tracks, secondary).is_some(),
        None => true,
    };

    primary_ok && secondary_ok
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

fn path_for_mpv(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
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

#[cfg(windows)]
fn bring_process_to_front(process_id: u32) {
    if let Some(hwnd) = find_window_for_process(process_id) {
        // SAFETY: hwnd is obtained from EnumWindows for the target process and is only used
        // with Windows window-management APIs that require raw handles.
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_RESTORE,
            );
            windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(windows)]
fn find_window_for_process(process_id: u32) -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct Search {
        process_id: u32,
        hwnd: HWND,
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
        // SAFETY: lparam is the Search pointer passed to EnumWindows for the duration of the call.
        let search = unsafe { &mut *(lparam as *mut Search) };
        let mut window_process_id = 0u32;
        // SAFETY: hwnd is provided by EnumWindows and the output pointer is valid here.
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut window_process_id);
        }
        // SAFETY: hwnd is provided by EnumWindows.
        if window_process_id == search.process_id && unsafe { IsWindowVisible(hwnd) } != 0 {
            search.hwnd = hwnd;
            return 0;
        }
        1
    }

    let mut search = Search {
        process_id,
        hwnd: std::ptr::null_mut(),
    };
    // SAFETY: enum_windows_proc follows the callback contract and lparam points to search.
    unsafe {
        EnumWindows(
            Some(enum_windows_proc),
            &mut search as *mut Search as LPARAM,
        );
    }
    if search.hwnd.is_null() {
        None
    } else {
        Some(search.hwnd)
    }
}

#[cfg(not(windows))]
fn bring_process_to_front(_process_id: u32) {}

#[cfg(test)]
mod tests {
    use super::{external_subtitle_count, parse_subtitle_tracks, track_list_has_video};
    use serde_json::json;

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
    fn counts_external_subtitles_in_track_list() {
        let tracks = json!([
            { "id": 1, "type": "video" },
            { "id": 1, "type": "sub", "external": false },
            { "id": 2, "type": "sub", "external": true },
            { "id": 3, "type": "sub", "external": true }
        ]);

        assert_eq!(external_subtitle_count(&tracks), 2);
    }
}
