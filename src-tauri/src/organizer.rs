use crate::domain::{
    AnimeSubMap, AnimeSubMapEpisode, BuildOrganizePlanRequest, CollisionAction, EpisodeMatch,
    FileOperationKind, FileOperationStatus, OrganizeExecutionResult, OrganizeMode, OrganizePlan,
    OrganizePlanItem, OrganizePlanSummary, OrganizeProgressEvent, SubtitleCandidate, SubtitleRole,
};
use crate::error::{AppError, AppResult};
use std::fs;
use std::path::{Path, PathBuf};

const APP_VERSION: &str = "0.1.0";

pub fn build_plan(request: BuildOrganizePlanRequest) -> AppResult<OrganizePlan> {
    let safe_project_name = sanitize_windows_file_name(&request.project_name);
    let project_output_dir =
        project_output_dir(&request.output_dir, &safe_project_name, &request.season);
    let mut items = Vec::new();
    let mut episodes = Vec::new();

    for episode_match in &request.matches {
        let map_episode = add_match_items(
            episode_match,
            &project_output_dir,
            &safe_project_name,
            &mut items,
        );
        episodes.push(map_episode);
    }

    if items.is_empty() {
        return Err(AppError::EmptyOrganizePlan);
    }

    let conflict_count = items.iter().filter(|item| item.collision).count();
    let video_count = items
        .iter()
        .filter(|item| item.kind == FileOperationKind::Video)
        .count();
    let subtitle_count = items
        .iter()
        .filter(|item| item.kind == FileOperationKind::Subtitle)
        .count();
    let map_file_path = project_output_dir.join("anime-sub-map.json");
    let project_map = AnimeSubMap {
        app_version: APP_VERSION.to_owned(),
        project_name: request.project_name,
        season: request.season,
        output_dir: project_output_dir.to_path_buf(),
        primary_language: request.primary_language,
        secondary_language: request.secondary_language,
        episodes,
    };

    Ok(OrganizePlan {
        project_name: project_map.project_name.to_owned(),
        season: project_map.season.to_owned(),
        output_dir: project_output_dir,
        mode: request.mode,
        items,
        has_conflicts: conflict_count > 0,
        map_file_exists: map_file_path.exists(),
        map_file_path,
        summary: OrganizePlanSummary {
            videos: video_count,
            subtitles: subtitle_count,
            conflicts: conflict_count,
        },
        project_map,
    })
}

pub fn execute_plan_with_progress<F>(
    plan: OrganizePlan,
    mut emit_progress: F,
) -> AppResult<OrganizeExecutionResult>
where
    F: FnMut(OrganizeProgressEvent) -> AppResult<()>,
{
    let total = plan.items.len();
    let mut result_items = Vec::with_capacity(total);
    emit_progress(OrganizeProgressEvent {
        total,
        processed: 0,
        current_episode_key: None,
        current_destination: None,
        status: FileOperationStatus::Planned,
        message: "整理任务已开始。".to_owned(),
    })?;

    for (index, mut item) in plan.items.into_iter().enumerate() {
        if item.collision && item.collision_action == CollisionAction::Skip {
            item.status = FileOperationStatus::Skipped;
            item.message = Some("目标文件已存在，已跳过。".to_owned());
            emit_item_progress(&mut emit_progress, total, index + 1, &item)?;
            result_items.push(item);
            continue;
        }

        execute_item(&mut item, plan.mode)?;
        emit_item_progress(&mut emit_progress, total, index + 1, &item)?;
        result_items.push(item);
    }

    fs::create_dir_all(&plan.output_dir)?;
    let map_json = serde_json::to_string_pretty(&plan.project_map)?;
    fs::write(&plan.map_file_path, map_json)?;
    emit_progress(OrganizeProgressEvent {
        total,
        processed: total,
        current_episode_key: None,
        current_destination: Some(plan.map_file_path.to_path_buf()),
        status: FileOperationStatus::Copied,
        message: "整理映射文件已写入。".to_owned(),
    })?;

    let message = match plan.mode {
        OrganizeMode::Copy => {
            "整理完成。原文件仍保留在原目录中。确认结果无误后，你可以手动删除原文件。"
        }
        OrganizeMode::Move => "移动整理完成。请检查输出目录中的结果。",
    }
    .to_owned();

    Ok(OrganizeExecutionResult {
        items: result_items,
        map_written: true,
        message,
    })
}

fn project_output_dir(base_output_dir: &Path, project_name: &str, season: &str) -> PathBuf {
    let folder_name = sanitize_windows_file_name(&format!("{project_name} {season}"));
    if base_output_dir
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(&folder_name))
    {
        return base_output_dir.to_path_buf();
    }
    base_output_dir.join(folder_name)
}

fn emit_item_progress<F>(
    emit_progress: &mut F,
    total: usize,
    processed: usize,
    item: &OrganizePlanItem,
) -> AppResult<()>
where
    F: FnMut(OrganizeProgressEvent) -> AppResult<()>,
{
    let message = item
        .message
        .to_owned()
        .unwrap_or_else(|| "文件处理完成。".to_owned());
    emit_progress(OrganizeProgressEvent {
        total,
        processed,
        current_episode_key: Some(item.episode_key.to_owned()),
        current_destination: Some(item.destination.to_path_buf()),
        status: item.status,
        message,
    })
}

fn add_match_items(
    episode_match: &EpisodeMatch,
    output_dir: &Path,
    project_name: &str,
    items: &mut Vec<OrganizePlanItem>,
) -> AnimeSubMapEpisode {
    let episode_key = episode_match.episode_key.to_owned();
    let video_destination = episode_match.video.as_ref().map(|video| {
        output_dir
            .join("videos")
            .join(format!("{project_name} {episode_key}.{}", video.extension))
    });
    if let (Some(video), Some(destination)) = (&episode_match.video, &video_destination) {
        items.push(plan_item(
            video.path.to_path_buf(),
            destination.to_path_buf(),
            FileOperationKind::Video,
            &episode_key,
            None,
            None,
        ));
    }

    let primary_destination = subtitle_destination(
        &episode_match.primary_subtitle,
        output_dir,
        project_name,
        &episode_key,
        SubtitleRole::Primary,
    );
    let secondary_destination = subtitle_destination(
        &episode_match.secondary_subtitle,
        output_dir,
        project_name,
        &episode_key,
        SubtitleRole::Secondary,
    );

    if let (Some(subtitle), Some(destination)) =
        (&episode_match.primary_subtitle, &primary_destination)
    {
        items.push(plan_item(
            subtitle.path.to_path_buf(),
            destination.to_path_buf(),
            FileOperationKind::Subtitle,
            &episode_key,
            Some(subtitle.language),
            Some(SubtitleRole::Primary),
        ));
    }
    if let (Some(subtitle), Some(destination)) =
        (&episode_match.secondary_subtitle, &secondary_destination)
    {
        items.push(plan_item(
            subtitle.path.to_path_buf(),
            destination.to_path_buf(),
            FileOperationKind::Subtitle,
            &episode_key,
            Some(subtitle.language),
            Some(SubtitleRole::Secondary),
        ));
    }

    AnimeSubMapEpisode {
        episode_key,
        video: video_destination.and_then(|path| relative_to_output(output_dir, &path)),
        primary_subtitle: primary_destination
            .and_then(|path| relative_to_output(output_dir, &path)),
        secondary_subtitle: secondary_destination
            .and_then(|path| relative_to_output(output_dir, &path)),
    }
}

fn subtitle_destination(
    subtitle: &Option<SubtitleCandidate>,
    output_dir: &Path,
    project_name: &str,
    episode_key: &str,
    role: SubtitleRole,
) -> Option<PathBuf> {
    let selected = subtitle.as_ref()?;
    let language = selected.language.as_str();
    let role_language = match role {
        SubtitleRole::Primary | SubtitleRole::Secondary | SubtitleRole::Candidate => language,
    };
    Some(output_dir.join("subs").join(role_language).join(format!(
        "{project_name} {episode_key}.{role_language}.{}",
        selected.extension
    )))
}

fn plan_item(
    source: PathBuf,
    destination: PathBuf,
    kind: FileOperationKind,
    episode_key: &str,
    language: Option<crate::domain::LanguageCode>,
    role: Option<SubtitleRole>,
) -> OrganizePlanItem {
    let collision = destination.exists();
    OrganizePlanItem {
        source,
        destination,
        kind,
        episode_key: episode_key.to_owned(),
        language,
        role,
        collision,
        collision_action: CollisionAction::Skip,
        status: FileOperationStatus::Planned,
        message: None,
    }
}

fn execute_item(item: &mut OrganizePlanItem, mode: OrganizeMode) -> AppResult<()> {
    if !item.source.is_file() {
        return Err(AppError::MissingFile(item.source.to_path_buf()));
    }

    if let Some(parent) = item.destination.parent() {
        fs::create_dir_all(parent)?;
    }

    if item.destination.exists() {
        match item.collision_action {
            CollisionAction::Skip => {
                item.status = FileOperationStatus::Skipped;
                item.message = Some("目标文件已存在，已跳过。".to_owned());
                return Ok(());
            }
            CollisionAction::Replace => fs::remove_file(&item.destination)?,
            CollisionAction::Rename => {
                item.destination = next_available_path(&item.destination);
            }
        }
    }

    match mode {
        OrganizeMode::Copy => {
            fs::copy(&item.source, &item.destination)?;
            item.status = FileOperationStatus::Copied;
            item.message = Some("已复制。".to_owned());
        }
        OrganizeMode::Move => {
            fs::copy(&item.source, &item.destination)?;
            fs::remove_file(&item.source)?;
            item.status = FileOperationStatus::Moved;
            item.message = Some("已移动。".to_owned());
        }
    }
    Ok(())
}

fn next_available_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());

    for index in 1..=999 {
        let candidate_name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = parent.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    parent.join(format!("{stem} (renamed)"))
}

fn relative_to_output(output_dir: &Path, path: &Path) -> Option<PathBuf> {
    path.strip_prefix(output_dir).ok().map(Path::to_path_buf)
}

pub fn sanitize_windows_file_name(input: &str) -> String {
    let replaced = input
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => character,
        })
        .collect::<String>();
    let trimmed = replaced.trim_matches([' ', '.']).trim().to_owned();
    let fallback = if trimmed.is_empty() {
        "Anime".to_owned()
    } else {
        trimmed
    };
    if is_reserved_windows_name(&fallback) {
        format!("{fallback}_")
    } else {
        fallback
    }
}

fn is_reserved_windows_name(value: &str) -> bool {
    let upper = value.to_ascii_uppercase();
    let name = upper.split('.').next().unwrap_or(&upper);
    matches!(
        name,
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

#[cfg(test)]
mod tests {
    use super::{build_plan, execute_plan_with_progress};
    use crate::domain::{
        BuildOrganizePlanRequest, CollisionAction, EpisodeKey, EpisodeMatch, LanguageCode,
        MatchStatus, OrganizeMode, ParseStatus, ScannedVideo, SubtitleCandidate, SubtitleRole,
    };
    use std::error::Error;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    #[test]
    fn copy_mode_keeps_originals_and_writes_map() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let source = temp.path().join("src");
        let output = temp.path().join("out");
        fs::create_dir_all(&source)?;
        let video = source.join("S01E01.mkv");
        let sub = source.join("S01E01.zh-Hans.ass");
        fs::write(&video, "video")?;
        fs::write(&sub, "sub")?;

        let plan = build_plan(request(&output, OrganizeMode::Copy, &video, &sub))?;
        let result = execute_plan_for_test(plan)?;

        assert!(video.exists());
        assert!(sub.exists());
        assert!(project_root(&output)
            .join("videos")
            .join("Jujutsu Kaisen S01E01.mkv")
            .exists());
        assert!(project_root(&output).join("anime-sub-map.json").exists());
        assert!(result.map_written);
        Ok(())
    }

    #[test]
    fn plan_output_root_includes_project_and_season_folder() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let source = temp.path().join("src");
        let output = temp.path().join("out");
        fs::create_dir_all(&source)?;
        let video = source.join("S01E01.mkv");
        let sub = source.join("S01E01.zh-Hans.ass");
        fs::write(&video, "video")?;
        fs::write(&sub, "sub")?;

        let plan = build_plan(request(&output, OrganizeMode::Copy, &video, &sub))?;

        assert_eq!(plan.output_dir, project_root(&output));
        assert_eq!(
            plan.map_file_path,
            project_root(&output).join("anime-sub-map.json")
        );
        assert!(plan.items.iter().any(|item| {
            item.destination
                == project_root(&output)
                    .join("videos")
                    .join("Jujutsu Kaisen S01E01.mkv")
        }));
        Ok(())
    }

    #[test]
    fn does_not_duplicate_project_folder_when_output_already_points_to_it(
    ) -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let source = temp.path().join("src");
        let output = project_root(temp.path());
        fs::create_dir_all(&source)?;
        let video = source.join("S01E01.mkv");
        let sub = source.join("S01E01.zh-Hans.ass");
        fs::write(&video, "video")?;
        fs::write(&sub, "sub")?;

        let plan = build_plan(request(&output, OrganizeMode::Copy, &video, &sub))?;

        assert_eq!(plan.output_dir, output);
        Ok(())
    }

    #[test]
    fn move_mode_removes_originals_after_successful_copy() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let source = temp.path().join("src");
        let output = temp.path().join("out");
        fs::create_dir_all(&source)?;
        let video = source.join("S01E01.mkv");
        let sub = source.join("S01E01.zh-Hans.ass");
        fs::write(&video, "video")?;
        fs::write(&sub, "sub")?;

        let plan = build_plan(request(&output, OrganizeMode::Move, &video, &sub))?;
        execute_plan_for_test(plan)?;

        assert!(!video.exists());
        assert!(!sub.exists());
        assert!(project_root(&output)
            .join("videos")
            .join("Jujutsu Kaisen S01E01.mkv")
            .exists());
        Ok(())
    }

    #[test]
    fn detects_collisions_without_overwriting() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let source = temp.path().join("src");
        let output = temp.path().join("out");
        fs::create_dir_all(&source)?;
        fs::create_dir_all(project_root(&output).join("videos"))?;
        let video = source.join("S01E01.mkv");
        let sub = source.join("S01E01.zh-Hans.ass");
        let destination = project_root(&output)
            .join("videos")
            .join("Jujutsu Kaisen S01E01.mkv");
        fs::write(&video, "video")?;
        fs::write(&sub, "sub")?;
        fs::write(&destination, "existing")?;

        let plan = build_plan(request(&output, OrganizeMode::Copy, &video, &sub))?;

        assert!(plan.has_conflicts);
        assert_eq!(fs::read_to_string(&destination)?, "existing");
        Ok(())
    }

    #[test]
    fn rename_collision_action_preserves_existing_file() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let source = temp.path().join("src");
        let output = temp.path().join("out");
        fs::create_dir_all(&source)?;
        fs::create_dir_all(project_root(&output).join("videos"))?;
        let video = source.join("S01E01.mkv");
        let sub = source.join("S01E01.zh-Hans.ass");
        fs::write(&video, "video")?;
        fs::write(&sub, "sub")?;
        fs::write(
            project_root(&output)
                .join("videos")
                .join("Jujutsu Kaisen S01E01.mkv"),
            "existing",
        )?;

        let mut plan = build_plan(request(&output, OrganizeMode::Copy, &video, &sub))?;
        for item in &mut plan.items {
            if item.collision {
                item.collision_action = CollisionAction::Rename;
            }
        }
        execute_plan_for_test(plan)?;

        assert!(project_root(&output)
            .join("videos")
            .join("Jujutsu Kaisen S01E01 (1).mkv")
            .exists());
        assert_eq!(
            fs::read_to_string(
                project_root(&output)
                    .join("videos")
                    .join("Jujutsu Kaisen S01E01.mkv")
            )?,
            "existing"
        );
        Ok(())
    }

    fn project_root(output: &Path) -> PathBuf {
        output.join("Jujutsu Kaisen S01")
    }

    fn execute_plan_for_test(
        plan: crate::domain::OrganizePlan,
    ) -> Result<crate::domain::OrganizeExecutionResult, crate::error::AppError> {
        execute_plan_with_progress(plan, |_| Ok(()))
    }

    fn request(
        output: &Path,
        mode: OrganizeMode,
        video: &Path,
        sub: &Path,
    ) -> BuildOrganizePlanRequest {
        BuildOrganizePlanRequest {
            project_name: "Jujutsu Kaisen".to_owned(),
            season: "S01".to_owned(),
            output_dir: output.to_path_buf(),
            matches: vec![episode_match(video, sub)],
            mode,
            primary_language: LanguageCode::ZhHans,
            secondary_language: Some(LanguageCode::Ja),
        }
    }

    fn episode_match(video: &Path, sub: &Path) -> EpisodeMatch {
        EpisodeMatch {
            episode: EpisodeKey::new(1, 1),
            episode_key: "S01E01".to_owned(),
            video: Some(ScannedVideo {
                path: video.to_path_buf(),
                file_name: "S01E01.mkv".to_owned(),
                extension: "mkv".to_owned(),
                file_size_bytes: 0,
                episode: Some(EpisodeKey::new(1, 1)),
                episode_key: Some("S01E01".to_owned()),
                confidence: 100,
                parse_status: ParseStatus::Accepted,
                parse_notes: Vec::new(),
                parse_candidates: Vec::new(),
            }),
            primary_subtitle: Some(SubtitleCandidate {
                path: sub.to_path_buf(),
                file_name: "S01E01.zh-Hans.ass".to_owned(),
                extension: "ass".to_owned(),
                language: LanguageCode::ZhHans,
                confidence: 100,
                role: SubtitleRole::Primary,
            }),
            secondary_subtitle: None,
            candidates: Vec::new(),
            status: MatchStatus::Matched,
            notes: Vec::new(),
        }
    }
}
