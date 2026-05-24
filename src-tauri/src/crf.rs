use crate::domain::{ParseSlotLabel, TokenCompoundKind, TokenFeatureKind, TokenFeatures};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrfModelMetadata {
    pub model_name: String,
    pub model_version: String,
    pub trained_at: Option<String>,
    pub training_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrfSlotModel {
    pub schema_version: u16,
    pub metadata: CrfModelMetadata,
    pub labels: Vec<ParseSlotLabel>,
    pub state_weights: Vec<CrfStateWeight>,
    pub transition_weights: Vec<CrfTransitionWeight>,
    pub start_weights: Vec<CrfStartWeight>,
    pub min_episode_score: f32,
    pub min_episode_margin: f32,
    pub episode_confidence: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrfStateWeight {
    pub label: ParseSlotLabel,
    pub feature: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrfTransitionWeight {
    pub from: ParseSlotLabel,
    pub to: ParseSlotLabel,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrfStartWeight {
    pub label: ParseSlotLabel,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrfTokenPrediction {
    pub index: usize,
    pub label: ParseSlotLabel,
    pub score: f32,
    pub margin: f32,
}

#[derive(Debug, Clone)]
pub struct CrfSlotTagger {
    model: CrfSlotModel,
}

impl CrfSlotTagger {
    pub fn load_optional(path: &Path) -> AppResult<Option<Self>> {
        if !path.try_exists()? {
            return Ok(None);
        }

        let content = std::fs::read_to_string(path)?;
        let model = serde_json::from_str::<CrfSlotModel>(&content)?;
        Self::from_model(model).map(Some)
    }

    pub fn from_model(model: CrfSlotModel) -> AppResult<Self> {
        validate_model(&model)?;
        Ok(Self { model })
    }

    pub fn episode_confidence(&self) -> u8 {
        self.model.episode_confidence.clamp(50, 92)
    }

    pub fn min_episode_score(&self) -> f32 {
        self.model.min_episode_score
    }

    pub fn min_episode_margin(&self) -> f32 {
        self.model.min_episode_margin
    }

    pub fn predict(&self, features: &[TokenFeatures]) -> Vec<CrfTokenPrediction> {
        if features.is_empty() {
            return Vec::new();
        }

        let labels = &self.model.labels;
        let mut previous_scores = labels
            .iter()
            .map(|label| self.start_weight(*label) + self.emission_score(*label, &features[0]))
            .collect::<Vec<_>>();
        let mut backpointers: Vec<Vec<usize>> = Vec::new();

        for feature in features.iter().skip(1) {
            let mut current_scores = Vec::with_capacity(labels.len());
            let mut current_backpointers = Vec::with_capacity(labels.len());

            for label in labels {
                let mut best: Option<(usize, f32)> = None;
                for (previous_index, previous_label) in labels.iter().enumerate() {
                    let score = previous_scores[previous_index]
                        + self.transition_weight(*previous_label, *label)
                        + self.emission_score(*label, feature);
                    if best.is_none_or(|(_, best_score)| score > best_score) {
                        best = Some((previous_index, score));
                    }
                }

                let (best_index, best_score) = match best {
                    Some(value) => value,
                    None => (0, self.emission_score(*label, feature)),
                };
                current_scores.push(best_score);
                current_backpointers.push(best_index);
            }

            previous_scores = current_scores;
            backpointers.push(current_backpointers);
        }

        let mut best_final: Option<(usize, f32)> = None;
        for (label_index, score) in previous_scores.iter().enumerate() {
            if best_final.is_none_or(|(_, best_score)| *score > best_score) {
                best_final = Some((label_index, *score));
            }
        }
        let Some((best_final_index, _)) = best_final else {
            return Vec::new();
        };

        let mut label_indexes = vec![0usize; features.len()];
        if let Some(last_index) = label_indexes.len().checked_sub(1) {
            label_indexes[last_index] = best_final_index;
            for step in (1..features.len()).rev() {
                let current_label_index = label_indexes[step];
                let previous_label_index = backpointers
                    .get(step - 1)
                    .and_then(|row| row.get(current_label_index))
                    .copied()
                    .unwrap_or(0);
                label_indexes[step - 1] = previous_label_index;
            }
        }

        features
            .iter()
            .zip(label_indexes)
            .map(|(feature, label_index)| {
                let label = labels
                    .get(label_index)
                    .copied()
                    .unwrap_or(ParseSlotLabel::Unknown);
                CrfTokenPrediction {
                    index: feature.index,
                    label,
                    score: self.emission_score(label, feature),
                    margin: self.local_margin(label, feature),
                }
            })
            .collect()
    }

    fn emission_score(&self, label: ParseSlotLabel, feature: &TokenFeatures) -> f32 {
        active_feature_names(feature)
            .iter()
            .map(|feature_name| self.state_weight(label, feature_name))
            .sum()
    }

    fn state_weight(&self, label: ParseSlotLabel, feature_name: &str) -> f32 {
        self.model
            .state_weights
            .iter()
            .filter(|weight| weight.label == label && weight.feature == feature_name)
            .map(|weight| weight.weight)
            .sum()
    }

    fn transition_weight(&self, from: ParseSlotLabel, to: ParseSlotLabel) -> f32 {
        self.model
            .transition_weights
            .iter()
            .filter(|weight| weight.from == from && weight.to == to)
            .map(|weight| weight.weight)
            .sum()
    }

    fn start_weight(&self, label: ParseSlotLabel) -> f32 {
        self.model
            .start_weights
            .iter()
            .filter(|weight| weight.label == label)
            .map(|weight| weight.weight)
            .sum()
    }

    fn local_margin(&self, label: ParseSlotLabel, feature: &TokenFeatures) -> f32 {
        let target_score = self.emission_score(label, feature);
        let mut second_best: Option<f32> = None;
        for other_label in &self.model.labels {
            if *other_label == label {
                continue;
            }
            let score = self.emission_score(*other_label, feature);
            if second_best.is_none_or(|best_score| score > best_score) {
                second_best = Some(score);
            }
        }
        target_score - second_best.unwrap_or(0.0)
    }
}

pub fn active_feature_names(feature: &TokenFeatures) -> Vec<String> {
    let mut names = Vec::new();
    names.push("bias".to_owned());
    names.push(format!("kind={}", feature_kind_name(feature.kind)));
    names.push(format!(
        "compound={}",
        feature
            .compound_kind
            .map(compound_kind_name)
            .unwrap_or("none")
    ));
    names.push(format!("lower={}", feature.lower));
    names.push(format!(
        "prev={}",
        feature.previous_token.as_deref().unwrap_or("<bos>")
    ));
    names.push(format!(
        "next={}",
        feature.next_token.as_deref().unwrap_or("<eos>")
    ));
    names.push(format!("bracketed={}", feature.is_bracketed));
    names.push(format!(
        "episode_marker={}",
        feature.is_episode_marker_context
    ));
    names.push(format!(
        "season_marker={}",
        feature.is_season_marker_context
    ));
    names.push(format!(
        "quality_or_source={}",
        feature.is_quality_or_source
    ));
    names.push(format!("language_token={}", feature.is_language_token));
    names.push(format!("special_token={}", feature.is_special_token));

    if let Some(value) = feature.number_value {
        names.push("number=present".to_owned());
        names.push(format!("number_bucket={}", number_bucket(value)));
        if value <= 200 {
            names.push(format!("number_value={value}"));
        }
    } else {
        names.push("number=absent".to_owned());
    }

    if let Some(width) = feature.number_width {
        names.push(format!("number_width={width}"));
    }

    names
}

fn validate_model(model: &CrfSlotModel) -> AppResult<()> {
    if model.schema_version != 1 {
        return Err(AppError::CrfModel(format!(
            "unsupported CRF schema version {}",
            model.schema_version
        )));
    }
    if model.labels.is_empty() {
        return Err(AppError::CrfModel(
            "CRF model must contain at least one label".to_owned(),
        ));
    }
    if !model.labels.contains(&ParseSlotLabel::Episode) {
        return Err(AppError::CrfModel(
            "CRF model labels must include episode".to_owned(),
        ));
    }
    Ok(())
}

fn feature_kind_name(kind: TokenFeatureKind) -> &'static str {
    match kind {
        TokenFeatureKind::Alpha => "alpha",
        TokenFeatureKind::Number => "number",
        TokenFeatureKind::Separator => "separator",
        TokenFeatureKind::Other => "other",
    }
}

fn compound_kind_name(kind: TokenCompoundKind) -> &'static str {
    match kind {
        TokenCompoundKind::SxxExx => "sxx_exx",
        TokenCompoundKind::VersionedEpisode => "versioned_episode",
        TokenCompoundKind::Resolution => "resolution",
        TokenCompoundKind::Codec => "codec",
        TokenCompoundKind::Source => "source",
        TokenCompoundKind::Language => "language",
        TokenCompoundKind::Hash => "hash",
    }
}

fn number_bucket(value: u32) -> &'static str {
    match value {
        0 => "zero",
        1..=12 => "small",
        13..=99 => "episode_range",
        100..=200 => "long_series",
        201..=479 => "large_noise",
        480 | 720 | 1080 | 1440 | 2160 | 4320 => "resolution",
        1900..=2099 => "year",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::{CrfSlotModel, CrfSlotTagger};
    use crate::domain::{ParseSlotLabel, TokenFeatureKind, TokenFeatures};

    #[test]
    fn predicts_episode_token_with_viterbi_sequence() -> Result<(), Box<dyn std::error::Error>> {
        let tagger = CrfSlotTagger::from_model(test_model()?)?;
        let features = vec![
            token(0, "Show", TokenFeatureKind::Alpha, None, Some("-")),
            token(1, "-", TokenFeatureKind::Separator, None, Some("7")),
            token(2, "7", TokenFeatureKind::Number, Some(7), Some("tail")),
            token(3, "tail", TokenFeatureKind::Alpha, None, None),
        ];

        let predictions = tagger.predict(&features);

        assert_eq!(predictions[2].label, ParseSlotLabel::Episode);
        assert!(predictions[2].score >= tagger.min_episode_score());
        Ok(())
    }

    fn test_model() -> Result<CrfSlotModel, serde_json::Error> {
        serde_json::from_str(
            r#"{
                "schemaVersion": 1,
                "metadata": {
                    "modelName": "unit-test",
                    "modelVersion": "0.1.0",
                    "trainedAt": null,
                    "trainingNote": null
                },
                "labels": ["unknown", "noise", "episode"],
                "stateWeights": [
                    { "label": "unknown", "feature": "bias", "weight": 0.2 },
                    { "label": "noise", "feature": "kind=separator", "weight": 3.0 },
                    { "label": "episode", "feature": "kind=number", "weight": 2.0 },
                    { "label": "episode", "feature": "number=present", "weight": 1.5 },
                    { "label": "episode", "feature": "number_bucket=small", "weight": 1.0 },
                    { "label": "episode", "feature": "prev=<bos>", "weight": -2.0 }
                ],
                "transitionWeights": [
                    { "from": "unknown", "to": "noise", "weight": 0.5 },
                    { "from": "noise", "to": "episode", "weight": 0.5 },
                    { "from": "episode", "to": "unknown", "weight": 0.5 }
                ],
                "startWeights": [
                    { "label": "unknown", "weight": 0.5 }
                ],
                "minEpisodeScore": 2.0,
                "minEpisodeMargin": 0.5,
                "episodeConfidence": 74
            }"#,
        )
    }

    fn token(
        index: usize,
        text: &str,
        kind: TokenFeatureKind,
        number_value: Option<u32>,
        next_token: Option<&str>,
    ) -> TokenFeatures {
        TokenFeatures {
            index,
            text: text.to_owned(),
            lower: text.to_lowercase(),
            kind,
            compound_kind: None,
            number_value,
            number_width: number_value.map(|value| value.to_string().len()),
            previous_token: None,
            next_token: next_token.map(ToOwned::to_owned),
            is_bracketed: false,
            is_episode_marker_context: false,
            is_season_marker_context: false,
            is_quality_or_source: false,
            is_language_token: false,
            is_special_token: false,
        }
    }
}
