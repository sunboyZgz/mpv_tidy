use crate::domain::{MpvLaunchRequest, MpvLaunchResult};
use crate::error::{AppError, AppResult};
use std::path::Path;
use std::process::Command;

pub fn launch(request: MpvLaunchRequest) -> AppResult<MpvLaunchResult> {
    if !request.video_path.is_file() {
        return Err(AppError::MissingFile(request.video_path));
    }

    let mut command = Command::new(&request.mpv_path);
    command.arg(&request.video_path);
    command.arg("--sub-auto=no");

    let mut subtitle_count = 0usize;
    if let Some(primary) = request.primary_subtitle {
        add_subtitle_arg(&mut command, &primary, &mut subtitle_count)?;
    }
    if let Some(secondary) = request.secondary_subtitle {
        add_subtitle_arg(&mut command, &secondary, &mut subtitle_count)?;
    }

    for arg in request.extra_args {
        if !arg.trim().is_empty() {
            command.arg(arg);
        }
    }

    let child = command
        .spawn()
        .map_err(|error| AppError::MpvLaunch(error.to_string()))?;

    Ok(MpvLaunchResult {
        process_id: child.id(),
        argument_count: 2 + subtitle_count + 1,
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

fn add_subtitle_arg(command: &mut Command, path: &Path, count: &mut usize) -> AppResult<()> {
    if *count >= 2 {
        return Ok(());
    }
    if !path.is_file() {
        return Err(AppError::MissingFile(path.to_path_buf()));
    }
    command.arg(format!("--sub-file={}", path.display()));
    *count += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::add_subtitle_arg;
    use std::error::Error;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn limits_mpv_subtitle_args_to_two() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let first = temp.path().join("a.ass");
        let second = temp.path().join("b.srt");
        let third = temp.path().join("c.srt");
        fs::write(&first, "a")?;
        fs::write(&second, "b")?;
        fs::write(&third, "c")?;
        let mut command = Command::new("mpv");
        let mut count = 0;

        add_subtitle_arg(&mut command, &first, &mut count)?;
        add_subtitle_arg(&mut command, &second, &mut count)?;
        add_subtitle_arg(&mut command, &third, &mut count)?;

        assert_eq!(count, 2);
        assert_eq!(command.get_args().count(), 2);
        Ok(())
    }
}
