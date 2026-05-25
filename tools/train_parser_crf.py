"""Train and export the parser CRF slot tagger.

Example:
    python tools/train_parser_crf.py \
      --input "%APPDATA%/com.mpvtidy.animesubtitlemanager/parser-training-samples.jsonl" \
              ".cache/generated-fixtures" \
      --output "%APPDATA%/com.mpvtidy.animesubtitlemanager/parser-crf-model.json" \
      --pickle-out .cache/parser-crf.pkl
"""

from __future__ import annotations

import argparse
import pickle
import random
import sys
from pathlib import Path

from parser_crf_common import (
    count_labels,
    evaluate_predictions,
    export_crf_model,
    load_training_dataset,
)


def main() -> int:
    args = parse_args()
    try:
        import sklearn_crfsuite
    except ImportError:
        print(
            "Missing dependency: sklearn-crfsuite. Install it with "
            "`pip install sklearn-crfsuite` in your training venv.",
            file=sys.stderr,
        )
        return 2

    dataset = load_training_dataset(args.input)
    train_x, train_y, dev_x, dev_y = split_dataset(
        dataset.features,
        dataset.labels,
        dev_ratio=args.dev_ratio,
        seed=args.seed,
    )

    crf = sklearn_crfsuite.CRF(
        algorithm="lbfgs",
        c1=args.c1,
        c2=args.c2,
        max_iterations=args.max_iterations,
        all_possible_transitions=True,
    )
    crf.fit(train_x, train_y)

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

    if args.pickle_out is not None:
        args.pickle_out.parent.mkdir(parents=True, exist_ok=True)
        with args.pickle_out.open("wb") as file:
            pickle.dump(crf, file)

    print_summary(
        sample_count=len(dataset.samples),
        train_count=len(train_x),
        dev_count=len(dev_x),
        labels=count_labels(dataset.labels),
        source_paths=dataset.source_paths,
        output=args.output,
        pickle_out=args.pickle_out,
    )

    if dev_x and dev_y:
        metrics = evaluate_predictions(dev_y, crf.predict(dev_x))
        print(
            "dev metrics: "
            f"token_accuracy={metrics['token_accuracy']:.3f}, "
            f"episode_precision={metrics['episode_precision']:.3f}, "
            f"episode_recall={metrics['episode_recall']:.3f}, "
            f"tokens={int(metrics['token_count'])}"
        )

    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Train parser CRF from parser-training-samples.jsonl and export JSON model.",
    )
    parser.add_argument(
        "--input",
        required=True,
        nargs="+",
        type=Path,
        help=(
            "Input JSONL training files or directories. Directories are searched "
            "recursively for *.jsonl files."
        ),
    )
    parser.add_argument("--output", required=True, type=Path, help="Output parser-crf-model.json.")
    parser.add_argument(
        "--pickle-out",
        type=Path,
        default=None,
        help="Optional sklearn-crfsuite pickle output for later re-export.",
    )
    parser.add_argument("--model-name", default="anime-parser-crf")
    parser.add_argument("--model-version", default="0.1.0")
    parser.add_argument("--training-note", default=None)
    parser.add_argument("--dev-ratio", type=float, default=0.2)
    parser.add_argument("--seed", type=int, default=20260524)
    parser.add_argument("--c1", type=float, default=0.1)
    parser.add_argument("--c2", type=float, default=0.1)
    parser.add_argument("--max-iterations", type=int, default=100)
    parser.add_argument("--min-abs-weight", type=float, default=0.05)
    parser.add_argument("--min-episode-score", type=float, default=2.5)
    parser.add_argument("--min-episode-margin", type=float, default=0.8)
    parser.add_argument("--episode-confidence", type=int, default=74)
    return parser.parse_args()


def split_dataset(
    features: list[list[dict[str, bool]]],
    labels: list[list[str]],
    *,
    dev_ratio: float,
    seed: int,
) -> tuple[
    list[list[dict[str, bool]]],
    list[list[str]],
    list[list[dict[str, bool]]],
    list[list[str]],
]:
    paired = list(zip(features, labels))
    random.Random(seed).shuffle(paired)
    if len(paired) < 3 or dev_ratio <= 0:
        train = paired
        dev = []
    else:
        dev_size = max(1, min(len(paired) - 1, round(len(paired) * dev_ratio)))
        dev = paired[:dev_size]
        train = paired[dev_size:]

    train_x = [item[0] for item in train]
    train_y = [item[1] for item in train]
    dev_x = [item[0] for item in dev]
    dev_y = [item[1] for item in dev]
    return train_x, train_y, dev_x, dev_y


def print_summary(
    *,
    sample_count: int,
    train_count: int,
    dev_count: int,
    labels: dict[str, int],
    source_paths: list[Path],
    output: Path,
    pickle_out: Path | None,
) -> None:
    print(f"samples={sample_count}, train={train_count}, dev={dev_count}")
    print(f"source_files={len(source_paths)}")
    for path in source_paths:
        print(f"  - {path}")
    print("labels=" + ", ".join(f"{label}:{count}" for label, count in labels.items()))
    print(f"exported_model={output}")
    if pickle_out is not None:
        print(f"pickle_model={pickle_out}")


if __name__ == "__main__":
    raise SystemExit(main())
