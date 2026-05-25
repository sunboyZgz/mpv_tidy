"""Export a trained sklearn-crfsuite pickle to the app JSON model format."""

from __future__ import annotations

import argparse
import pickle
from pathlib import Path

from parser_crf_common import export_crf_model


def main() -> int:
    args = parse_args()
    with args.input.open("rb") as file:
        crf = pickle.load(file)

    export_crf_model(
        crf,
        args.output,
        model_name=args.model_name,
        model_version=args.model_version,
        training_note=args.training_note,
        min_abs_weight=args.min_abs_weight,
        min_episode_score=args.min_episode_score,
        min_episode_margin=args.min_episode_margin,
        episode_confidence=args.episode_confidence,
    )
    print(f"exported_model={args.output}")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Export sklearn-crfsuite pickle to parser-crf-model.json.",
    )
    parser.add_argument("--input", required=True, type=Path, help="Input .pkl model.")
    parser.add_argument("--output", required=True, type=Path, help="Output parser-crf-model.json.")
    parser.add_argument("--model-name", default="anime-parser-crf")
    parser.add_argument("--model-version", default="0.1.0")
    parser.add_argument("--training-note", default=None)
    parser.add_argument("--min-abs-weight", type=float, default=0.05)
    parser.add_argument("--min-episode-score", type=float, default=2.5)
    parser.add_argument("--min-episode-margin", type=float, default=0.8)
    parser.add_argument("--episode-confidence", type=int, default=74)
    return parser.parse_args()


if __name__ == "__main__":
    raise SystemExit(main())
