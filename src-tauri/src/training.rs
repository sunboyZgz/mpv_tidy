use crate::domain::{ParseTrainingSample, SaveParseTrainingSampleRequest};
use crate::error::AppResult;
use crate::parser;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn save_parse_training_sample(
    training_path: &Path,
    request: SaveParseTrainingSampleRequest,
) -> AppResult<ParseTrainingSample> {
    if let Some(parent) = training_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let sample = parser::build_training_sample(
        &request.path,
        request.confirmed_episode,
        request.note,
        now_unix(),
    );
    let line = serde_json::to_string(&sample)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(training_path)?;
    writeln!(file, "{line}")?;
    Ok(sample)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::save_parse_training_sample;
    use crate::domain::{EpisodeKey, ParseSlotLabel, SaveParseTrainingSampleRequest};
    use std::error::Error;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn saves_user_confirmation_as_jsonl_training_sample() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let training_path = temp.path().join("parser-training-samples.jsonl");

        let sample = save_parse_training_sample(
            &training_path,
            SaveParseTrainingSampleRequest {
                path: PathBuf::from("Show - 03v2 [1080p].mkv"),
                confirmed_episode: Some(EpisodeKey::new(1, 3)),
                note: Some("manual correction".to_owned()),
            },
        )?;

        assert_eq!(sample.schema_version, 1);
        assert!(sample
            .tokens
            .iter()
            .any(|token| token.label == ParseSlotLabel::Episode));

        let content = fs::read_to_string(training_path)?;
        assert_eq!(content.lines().count(), 1);
        assert!(content.contains("manual correction"));
        Ok(())
    }
}
