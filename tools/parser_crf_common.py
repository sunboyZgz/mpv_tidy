"""Shared helpers for parser CRF training/export tools.

These scripts are development-only. The Tauri application does not import or
execute Python at runtime.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable


ALL_LABELS = [
    "unknown",
    "noise",
    "episode",
    "season",
    "version",
    "hash",
    "resolution",
    "codec",
    "source",
    "language",
    "title",
    "special",
]


COMPOUND_NAME_MAP = {
    None: "none",
    "sxxExx": "sxx_exx",
    "versionedEpisode": "versioned_episode",
    "resolution": "resolution",
    "codec": "codec",
    "source": "source",
    "language": "language",
    "hash": "hash",
}


@dataclass(frozen=True)
class TrainingDataset:
    samples: list[dict[str, Any]]
    features: list[list[dict[str, bool]]]
    labels: list[list[str]]
    source_paths: list[Path]


def load_training_dataset(inputs: Path | Iterable[Path]) -> TrainingDataset:
    paths = resolve_training_paths([inputs] if isinstance(inputs, Path) else inputs)
    samples: list[dict[str, Any]] = []
    features: list[list[dict[str, bool]]] = []
    labels: list[list[str]] = []

    for path in paths:
        with path.open("r", encoding="utf-8-sig") as file:
            for line_number, line in enumerate(file, start=1):
                stripped = line.strip()
                if not stripped:
                    continue
                sample = json.loads(stripped)
                tokens = sample.get("tokens")
                if not isinstance(tokens, list) or not tokens:
                    raise ValueError(
                        f"{path}:{line_number}: sample must contain non-empty tokens"
                    )
                sample_features = [token_feature_names(token) for token in tokens]
                sample_labels = [normalize_label(token.get("label")) for token in tokens]
                samples.append(sample)
                features.append(sample_features)
                labels.append(sample_labels)

    if not samples:
        joined_paths = ", ".join(str(path) for path in paths)
        raise ValueError(f"no training samples found in {joined_paths}")

    return TrainingDataset(
        samples=samples,
        features=features,
        labels=labels,
        source_paths=paths,
    )


def resolve_training_paths(inputs: Iterable[Path]) -> list[Path]:
    paths: list[Path] = []
    for input_path in inputs:
        if input_path.is_dir():
            paths.extend(
                sorted(path for path in input_path.rglob("*.jsonl") if path.is_file())
            )
        elif input_path.is_file():
            paths.append(input_path)
        else:
            raise FileNotFoundError(f"training input does not exist: {input_path}")

    deduped: list[Path] = []
    seen: set[Path] = set()
    for path in paths:
        resolved = path.resolve()
        if resolved not in seen:
            seen.add(resolved)
            deduped.append(path)

    if not deduped:
        raise FileNotFoundError("no .jsonl training files found")

    return deduped


def token_feature_names(token: dict[str, Any]) -> dict[str, bool]:
    features = token.get("features")
    if not isinstance(features, dict):
        raise ValueError("token must contain a features object")

    names = {
        "bias": True,
        f"kind={features.get('kind', 'other')}": True,
        f"compound={compound_name(features.get('compoundKind'))}": True,
        f"lower={features.get('lower', '')}": True,
        f"prev={features.get('previousToken') or '<bos>'}": True,
        f"next={features.get('nextToken') or '<eos>'}": True,
        f"bracketed={bool_text(features.get('isBracketed'))}": True,
        f"episode_marker={bool_text(features.get('isEpisodeMarkerContext'))}": True,
        f"season_marker={bool_text(features.get('isSeasonMarkerContext'))}": True,
        f"quality_or_source={bool_text(features.get('isQualityOrSource'))}": True,
        f"language_token={bool_text(features.get('isLanguageToken'))}": True,
        f"special_token={bool_text(features.get('isSpecialToken'))}": True,
    }

    number_value = features.get("numberValue")
    if number_value is None:
        names["number=absent"] = True
    else:
        value = int(number_value)
        names["number=present"] = True
        names[f"number_bucket={number_bucket(value)}"] = True
        if value <= 200:
            names[f"number_value={value}"] = True

    number_width = features.get("numberWidth")
    if number_width is not None:
        names[f"number_width={int(number_width)}"] = True

    return names


def export_crf_model(
    crf: Any,
    output_path: Path,
    *,
    model_name: str,
    model_version: str,
    training_note: str | None,
    min_abs_weight: float,
    min_episode_score: float,
    min_episode_margin: float,
    episode_confidence: int,
) -> None:
    labels = model_labels(crf)
    state_weights = [
        {"label": label, "feature": feature, "weight": round(float(weight), 6)}
        for (feature, label), weight in sorted(crf.state_features_.items())
        if abs(float(weight)) >= min_abs_weight
    ]
    transition_weights = [
        {"from": from_label, "to": to_label, "weight": round(float(weight), 6)}
        for (from_label, to_label), weight in sorted(crf.transition_features_.items())
        if abs(float(weight)) >= min_abs_weight
    ]

    model = {
        "schemaVersion": 1,
        "metadata": {
            "modelName": model_name,
            "modelVersion": model_version,
            "trainedAt": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "trainingNote": training_note,
        },
        "labels": labels,
        "stateWeights": state_weights,
        "transitionWeights": transition_weights,
        "startWeights": [],
        "minEpisodeScore": min_episode_score,
        "minEpisodeMargin": min_episode_margin,
        "episodeConfidence": episode_confidence,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(model, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )


def model_labels(crf: Any) -> list[str]:
    observed = {normalize_label(label) for label in getattr(crf, "classes_", [])}
    labels = [label for label in ALL_LABELS if label in observed or label == "episode"]
    if "episode" not in labels:
        labels.append("episode")
    remaining = sorted(observed.difference(labels))
    return labels + remaining


def evaluate_predictions(expected: list[list[str]], predicted: list[list[str]]) -> dict[str, float]:
    total = 0
    correct = 0
    episode_true = 0
    episode_predicted = 0
    episode_correct = 0

    for expected_sequence, predicted_sequence in zip(expected, predicted):
        for expected_label, predicted_label in zip(expected_sequence, predicted_sequence):
            total += 1
            if expected_label == predicted_label:
                correct += 1
            if expected_label == "episode":
                episode_true += 1
            if predicted_label == "episode":
                episode_predicted += 1
            if expected_label == "episode" and predicted_label == "episode":
                episode_correct += 1

    accuracy = correct / total if total else 0.0
    episode_precision = episode_correct / episode_predicted if episode_predicted else 0.0
    episode_recall = episode_correct / episode_true if episode_true else 0.0
    return {
        "token_accuracy": accuracy,
        "episode_precision": episode_precision,
        "episode_recall": episode_recall,
        "token_count": float(total),
    }


def normalize_label(value: Any) -> str:
    if not isinstance(value, str):
        return "unknown"
    return value if value in ALL_LABELS else "unknown"


def bool_text(value: Any) -> str:
    return "true" if bool(value) else "false"


def compound_name(value: Any) -> str:
    return COMPOUND_NAME_MAP.get(value, "none")


def number_bucket(value: int) -> str:
    if value == 0:
        return "zero"
    if 1 <= value <= 12:
        return "small"
    if 13 <= value <= 99:
        return "episode_range"
    if 100 <= value <= 200:
        return "long_series"
    if 201 <= value <= 479:
        return "large_noise"
    if value in {480, 720, 1080, 1440, 2160, 4320}:
        return "resolution"
    if 1900 <= value <= 2099:
        return "year"
    return "other"


def count_labels(label_sequences: Iterable[Iterable[str]]) -> dict[str, int]:
    counts = {label: 0 for label in ALL_LABELS}
    for sequence in label_sequences:
        for label in sequence:
            counts[normalize_label(label)] = counts.get(normalize_label(label), 0) + 1
    return {label: count for label, count in counts.items() if count > 0}
