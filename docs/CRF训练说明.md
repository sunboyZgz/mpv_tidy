# CRF 训练说明

这份文档面向“没有模型经验”的开发者。当前应用不会要求普通用户安装 Python、模型服务或额外运行环境。Python 只用于开发阶段离线训练，训练完成后导出一个 `parser-crf-model.json`，桌面应用启动扫描时会用 Rust 在本地读取这个文件并推理。

## 1. 现在 CRF 在应用里做什么

CRF 不是接管整个 parser。主链路仍然是：

1. 强规则解析：例如 `S01E01`、`第01話`、`01v2`。
2. 批量模板归纳：同一目录下找稳定的集数字段。
3. 如果前两步没有得到 accepted 结果，才调用 CRF。
4. CRF 只判断 token 标签，例如哪个 token 更像 `episode`。
5. 最终是否接受仍由 Rust parser 融合候选、置信度和人工确认机制决定。

如果没有模型文件，应用会自动回到确定性 parser，不影响扫描。

## 2. 训练数据从哪里来

当你在前端手动修正某个视频或字幕的集数时，应用会在本机 app data 目录追加一条 JSONL 训练样本：

```text
%APPDATA%\com.mpvtidy.animesubtitlemanager\parser-training-samples.jsonl
```

JSONL 的意思是“一行一个 JSON”。每一行大致包含：

```json
{
  "schemaVersion": 1,
  "source": "userConfirmation",
  "path": "D:/Anime/Show/Mysteryxx7xx.mkv",
  "confirmedEpisode": { "season": 1, "episode": 7 },
  "tokens": [
    {
      "features": {
        "index": 1,
        "text": "7",
        "kind": "number",
        "numberValue": 7
      },
      "label": "episode"
    }
  ]
}
```

你不需要手写这些样本。正常使用应用，遇到低置信或识别错的文件时手动修正，样本就会积累。

## 3. 需要准备多少数据

第一版不要追求“大模型”。建议这样收集：

- 每种常见命名模板至少 10 到 20 条。
- 错误高发模板优先收集，比如无分隔、多个数字、特殊版本号。
- 一定要保留反例，例如 `1080p`、`10bit`、`H.264`、日期、hash、page、segment。
- 同一部作品不要刷太多重复样本，否则模型会记作品名而不是学结构。

比较健康的第一批数据量：200 到 500 条真实文件名。

## 4. 离线训练的基本流程

在开发机上准备 Python 环境：

```powershell
python -m venv .venv-crf
.\.venv-crf\Scripts\Activate.ps1
pip install sklearn-crfsuite
```

训练脚本的核心工作只有三件事：

1. 读取 `parser-training-samples.jsonl`。
2. 把每个 token 的特征转成 CRF 可读的 feature 字典。
3. 训练后把权重导出成应用可读的 `parser-crf-model.json`。

特征名必须和 Rust 里的 `active_feature_names()` 保持一致，例如：

```text
bias
kind=number
compound=versioned_episode
lower=7
prev=-
next=xx
number=present
number_bucket=small
number_width=1
episode_marker=false
quality_or_source=false
```

当前 tokenizer 是三层设计：

1. 通用字符分类：数字、文字、分隔符、其他。`!`、`$`、`@`、`#`、`~` 这类未知符号默认作为分隔边界，不参与 episode 语义。
2. 媒体复合结构识别：对 `S01E01`、`01v2`、`1080p`、`4K`、`H.264`、`x265`、`WEB-DL`、`zh-hans`、`ja-jp`、`2D6390A9` 这类结构生成 `compound=...` 特征。
3. 语义槽位判断：deterministic parser 和 CRF 再判断某个数字是 episode、resolution、codec、hash、version 还是 noise。

这意味着 `Show$01@1080p.mkv` 不会因为 `$` 或 `@` 崩掉。parser 仍然能看到 `Show / $ / 01 / @ / 1080 / p`，然后结合模板和 CRF 判断 `01`。

## 5. 模型文件格式

应用读取的模型文件名固定为：

```text
parser-crf-model.json
```

放置路径：

```text
%APPDATA%\com.mpvtidy.animesubtitlemanager\parser-crf-model.json
```

最小结构如下：

```json
{
  "schemaVersion": 1,
  "metadata": {
    "modelName": "anime-parser-crf",
    "modelVersion": "0.1.0",
    "trainedAt": "2026-05-24",
    "trainingNote": "first local CRF model"
  },
  "labels": ["unknown", "noise", "episode", "season", "version", "hash", "resolution", "codec", "source", "language", "title", "special"],
  "stateWeights": [
    { "label": "episode", "feature": "kind=number", "weight": 2.0 }
  ],
  "transitionWeights": [
    { "from": "noise", "to": "episode", "weight": 0.4 }
  ],
  "startWeights": [
    { "label": "unknown", "weight": 0.2 }
  ],
  "minEpisodeScore": 2.5,
  "minEpisodeMargin": 0.8,
  "episodeConfidence": 74
}
```

字段含义：

- `stateWeights`：某个 token 特征对某个标签的加分。
- `transitionWeights`：相邻标签之间的加分，例如 `noise -> episode`。
- `startWeights`：第一个 token 的标签加分。
- `minEpisodeScore`：episode token 的最低分，低于它不采用。
- `minEpisodeMargin`：episode 比其他标签至少高多少，低于它视为不稳定。
- `episodeConfidence`：CRF 候选进入 parser 后使用的置信度，建议 70 到 80，不要高过强规则。

## 6. 训练脚本伪代码

下面是流程示意，不是应用运行时依赖：

```python
import json
import sklearn_crfsuite

def number_bucket(value):
    if value == 0:
        return "zero"
    if 1 <= value <= 12:
        return "small"
    if 13 <= value <= 99:
        return "episode_range"
    if 100 <= value <= 200:
        return "long_series"
    if value in [480, 720, 1080, 1440, 2160, 4320]:
        return "resolution"
    if 1900 <= value <= 2099:
        return "year"
    return "other"

def compound_name(value):
    mapping = {
        None: "none",
        "sxxExx": "sxx_exx",
        "versionedEpisode": "versioned_episode",
        "resolution": "resolution",
        "codec": "codec",
        "source": "source",
        "language": "language",
        "hash": "hash",
    }
    return mapping.get(value, "none")

def token_features(token):
    f = token["features"]
    names = {
        "bias": True,
        f"kind={f['kind']}": True,
        f"compound={compound_name(f.get('compoundKind'))}": True,
        f"lower={f['lower']}": True,
        f"prev={f.get('previousToken') or '<bos>'}": True,
        f"next={f.get('nextToken') or '<eos>'}": True,
        f"bracketed={str(f['isBracketed']).lower()}": True,
        f"episode_marker={str(f['isEpisodeMarkerContext']).lower()}": True,
        f"season_marker={str(f['isSeasonMarkerContext']).lower()}": True,
        f"quality_or_source={str(f['isQualityOrSource']).lower()}": True,
        f"language_token={str(f['isLanguageToken']).lower()}": True,
        f"special_token={str(f['isSpecialToken']).lower()}": True,
    }
    if f.get("numberValue") is None:
        names["number=absent"] = True
    else:
        value = f["numberValue"]
        names["number=present"] = True
        names[f"number_bucket={number_bucket(value)}"] = True
        if value <= 200:
            names[f"number_value={value}"] = True
    if f.get("numberWidth") is not None:
        names[f"number_width={f['numberWidth']}"] = True
    return names

samples = []
with open("parser-training-samples.jsonl", "r", encoding="utf-8") as file:
    for line in file:
        samples.append(json.loads(line))

X = [[token_features(token) for token in sample["tokens"]] for sample in samples]
y = [[token["label"] for token in sample["tokens"]] for sample in samples]

crf = sklearn_crfsuite.CRF(
    algorithm="lbfgs",
    c1=0.1,
    c2=0.1,
    max_iterations=100,
    all_possible_transitions=True,
)
crf.fit(X, y)
```

训练完成后，需要把 `crf.state_features_`、`crf.transition_features_` 和起始权重转换成上面的 JSON 格式。不同 CRF 库导出的内部字段名字可能不同，所以导出脚本要和你选择的库绑定。

## 7. 如何验证模型有没有生效

1. 把 `parser-crf-model.json` 放到 app data 目录。
2. 启动应用并扫描一个强规则和模板归纳都识别不了的文件名。
3. 如果 CRF 参与，详情里的候选来源会出现 `crf`，说明文本会包含 `CRF slot tagger selected token ...`。
4. 如果 CRF 没有把握，结果仍会保持低置信、歧义或拒识。

## 8. 版本管理建议

每次训练都记录：

- 训练样本文件备份。
- 模型版本号，例如 `0.1.0`、`0.1.1`。
- 大致训练数据量。
- 主要新增覆盖的命名模板。
- 已知失败案例。

模型表现不好时，直接删除 `parser-crf-model.json`，应用会回到纯确定性 parser。

## 9. 不要过早做的事

- 不要让 CRF 覆盖高置信强规则结果。
- 不要把 `episodeConfidence` 调到 90 以上。
- 不要只用正例训练，反例同样重要。
- 不要把模型训练放进普通用户的应用流程里。
- 不要让用户感知“必须有模型才能用”。

当前设计的目标是：轻量、本地、可复现、可解释。CRF 只是低置信场景的辅助证据，不是最终裁判。
