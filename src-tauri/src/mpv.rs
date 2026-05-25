use crate::domain::{MpvLaunchRequest, MpvLaunchResult};
use crate::error::{AppError, AppResult};
use serde_json::json;
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static NEXT_IPC_REQUEST_ID: AtomicI64 = AtomicI64::new(1);

#[derive(Default)]
pub struct MpvController {
    session: Mutex<Option<MpvSession>>,
}

struct MpvSession {
    child: Child,
    process_id: u32,
    current_video_path: PathBuf,
    ipc_pipe_path: String,
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
        if existing.child.try_wait()?.is_none() {
            if same_path(&existing.current_video_path, &request.video_path) {
                bring_process_to_front(existing.process_id);
                return Ok(MpvLaunchResult {
                    process_id: existing.process_id,
                    argument_count: 0,
                    reused_existing: true,
                    switched_video: false,
                });
            }

            if let Err(error) = switch_running_mpv(existing, &request) {
                *session = None;
                return Err(error);
            }
            existing.current_video_path = request.video_path.to_path_buf();
            bring_process_to_front(existing.process_id);
            return Ok(MpvLaunchResult {
                process_id: existing.process_id,
                argument_count: 0,
                reused_existing: true,
                switched_video: true,
            });
        }
    }

    let next_session = spawn_mpv(request)?;
    let result = MpvLaunchResult {
        process_id: next_session.process_id,
        argument_count: next_session_argument_count(),
        reused_existing: false,
        switched_video: false,
    };
    *session = Some(next_session);
    Ok(result)
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

fn spawn_mpv(request: MpvLaunchRequest) -> AppResult<MpvSession> {
    let ipc_pipe_path = next_ipc_pipe_path();
    let mut command = Command::new(&request.mpv_path);
    command.arg(&request.video_path);
    command.arg("--idle=yes");
    command.arg("--sub-auto=no");
    command.arg("--sid=no");
    command.arg("--secondary-sid=no");
    command.arg(format!("--input-ipc-server={ipc_pipe_path}"));

    for arg in &request.extra_args {
        if !arg.trim().is_empty() {
            command.arg(arg);
        }
    }

    let child = command
        .spawn()
        .map_err(|error| AppError::MpvLaunch(error.to_string()))?;
    let process_id = child.id();
    let session = MpvSession {
        child,
        process_id,
        current_video_path: request.video_path.to_path_buf(),
        ipc_pipe_path,
    };
    thread::sleep(Duration::from_millis(180));
    apply_subtitle_selection(&session.ipc_pipe_path, &request)?;
    Ok(session)
}

fn switch_running_mpv(session: &MpvSession, request: &MpvLaunchRequest) -> AppResult<()> {
    send_ipc_command(
        &session.ipc_pipe_path,
        json!(["loadfile", path_for_mpv(&request.video_path), "replace"]),
    )?;
    thread::sleep(Duration::from_millis(220));

    apply_subtitle_selection(&session.ipc_pipe_path, request)?;
    for command in runtime_property_commands(&request.extra_args) {
        send_ipc_command(&session.ipc_pipe_path, command)?;
    }
    Ok(())
}

fn apply_subtitle_selection(pipe_path: &str, request: &MpvLaunchRequest) -> AppResult<()> {
    send_ipc_command(pipe_path, json!(["set_property", "sid", "no"]))?;
    send_ipc_command(pipe_path, json!(["set_property", "secondary-sid", "no"]))?;
    if let Some(primary) = &request.primary_subtitle {
        send_ipc_command(
            pipe_path,
            json!(["sub-add", path_for_mpv(primary), "select", "primary"]),
        )?;
    }
    if let Some(secondary) = &request.secondary_subtitle {
        send_ipc_command(
            pipe_path,
            json!(["sub-add", path_for_mpv(secondary), "auto", "secondary"]),
        )?;
    }

    thread::sleep(Duration::from_millis(120));
    let tracks = send_ipc_request(pipe_path, json!(["get_property", "track-list"]))?;

    if let Some(primary) = &request.primary_subtitle {
        let Some(track_id) = find_external_subtitle_track_id(&tracks, primary) else {
            return Err(AppError::MpvLaunch(format!(
                "MPV 未能载入主字幕：{}",
                primary.display()
            )));
        };
        send_ipc_command(pipe_path, json!(["set_property", "sid", track_id]))?;
    }
    if let Some(secondary) = &request.secondary_subtitle {
        let Some(track_id) = find_external_subtitle_track_id(&tracks, secondary) else {
            return Err(AppError::MpvLaunch(format!(
                "MPV 未能载入副字幕：{}",
                secondary.display()
            )));
        };
        send_ipc_command(
            pipe_path,
            json!(["set_property", "secondary-sid", track_id]),
        )?;
    }
    Ok(())
}

fn find_external_subtitle_track_id(track_list: &Value, subtitle_path: &Path) -> Option<i64> {
    let wanted = [
        normalized_mpv_path(subtitle_path),
        normalized_path_text(&path_for_mpv(subtitle_path)),
    ];
    track_list.as_array()?.iter().find_map(|track| {
        if track.get("type")?.as_str()? != "sub" {
            return None;
        }
        let filename = track
            .get("external-filename")
            .or_else(|| track.get("filename"))?
            .as_str()?;
        if wanted.contains(&normalized_path_text(filename)) {
            track.get("id")?.as_i64()
        } else {
            None
        }
    })
}

fn runtime_property_commands(extra_args: &[String]) -> Vec<serde_json::Value> {
    let mut commands = Vec::new();
    for arg in extra_args {
        if let Some(value) = arg.strip_prefix("--sub-delay=") {
            if let Ok(delay) = value.parse::<f64>() {
                commands.push(json!(["set_property", "sub-delay", delay]));
            }
        }
    }
    commands
}

fn send_ipc_command(pipe_path: &str, command: Value) -> AppResult<()> {
    send_ipc_request(pipe_path, command).map(|_| ())
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
            return Err(AppError::MpvLaunch("MPV IPC 连接已关闭".to_owned()));
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
            return Err(AppError::MpvLaunch(format!("MPV IPC 命令失败：{error}")));
        }
        return Ok(value.get("data").cloned().unwrap_or(Value::Null));
    }
}

fn open_ipc_pipe(pipe_path: &str) -> AppResult<std::fs::File> {
    let mut last_error = None;
    for _ in 0..30 {
        match OpenOptions::new().read(true).write(true).open(pipe_path) {
            Ok(file) => return Ok(file),
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Err(AppError::MpvLaunch(
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "无法连接 MPV IPC 管道".to_owned()),
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

fn normalized_mpv_path(path: &Path) -> String {
    match path.canonicalize() {
        Ok(canonical) => path_for_mpv(&canonical).to_ascii_lowercase(),
        Err(_) => path_for_mpv(path).to_ascii_lowercase(),
    }
}

fn normalized_path_text(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn next_session_argument_count() -> usize {
    0
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
    use super::{find_external_subtitle_track_id, runtime_property_commands, same_path};
    use serde_json::json;
    use std::error::Error;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detects_same_paths_without_spawning_second_player() {
        assert!(same_path(
            &std::path::PathBuf::from("S01E01.mkv"),
            &std::path::PathBuf::from("S01E01.mkv")
        ));
    }

    #[test]
    fn converts_sub_delay_args_to_runtime_ipc_properties() {
        let commands = runtime_property_commands(&["--sub-delay=1.5".to_owned()]);
        assert_eq!(commands.len(), 1);
    }

    #[test]
    fn finds_external_subtitle_track_by_path_instead_of_embedded_track_id(
    ) -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let primary = temp.path().join("primary.ass");
        fs::write(&primary, "sub")?;
        let tracks = json!([
            { "id": 1, "type": "sub", "title": "embedded default" },
            { "id": 3, "type": "sub", "external-filename": primary.to_string_lossy().replace('\\', "/") }
        ]);

        assert_eq!(find_external_subtitle_track_id(&tracks, &primary), Some(3));
        Ok(())
    }
}
