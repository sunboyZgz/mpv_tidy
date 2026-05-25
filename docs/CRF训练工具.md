# CRF 训练工具使用说明

本目录下的脚本只给开发者离线训练模型用。普通用户运行桌面应用时不需要 Python、不需要训练环境、不需要服务。

## 1. 安装训练依赖

在项目根目录执行：

```powershell
python -m venv .venv-crf
.\.venv-crf\Scripts\Activate.ps1
pip install -r tools/requirements-crf.txt
```

## 2. 找到训练数据

应用会把手动修正产生的样本追加到：

```text
%APPDATA%\com.mpvtidy.animesubtitlemanager\parser-training-samples.jsonl
```

这个文件是一行一个 JSON 样本。你正常使用应用、遇到识别错的文件就手动修正，训练数据就会慢慢积累。

## 3. 一步训练并导出模型

推荐先输出到 `.cache`，确认效果后再复制到 app data：

```powershell
python tools/train_parser_crf.py `
  --input "$env:APPDATA\com.mpvtidy.animesubtitlemanager\parser-training-samples.jsonl" `
  --output ".cache\parser-crf-model.json" `
  --pickle-out ".cache\parser-crf.pkl" `
  --model-version "0.1.0" `
  --training-note "first local parser CRF"
```

`--input` 可以同时传多个文件，也可以传目录。目录会递归读取其中所有 `*.jsonl`：

```powershell
python tools/train_parser_crf.py `
  --input `
    "$env:APPDATA\com.mpvtidy.animesubtitlemanager\parser-training-samples.jsonl" `
    ".cache\generated-training-data" `
    ".cache\manual-extra-samples.jsonl" `
  --output ".cache\parser-crf-model.json" `
  --pickle-out ".cache\parser-crf.pkl"
```

脚本会输出：

- 样本数、训练集数量、验证集数量。
- 实际读取到的 JSONL 文件列表。
- 各标签数量。
- 简单验证指标：token accuracy、episode precision、episode recall。
- 应用可读模型：`.cache/parser-crf-model.json`。
- 可选 pickle：`.cache/parser-crf.pkl`，方便之后不用重新训练也能重新导出 JSON。

## 4. 只重新导出 JSON

如果你已经有 `.cache/parser-crf.pkl`，只是想调整阈值或版本号：

```powershell
python tools/export_parser_crf_model.py `
  --input ".cache\parser-crf.pkl" `
  --output ".cache\parser-crf-model.json" `
  --model-version "0.1.1" `
  --min-episode-score 2.5 `
  --min-episode-margin 0.8 `
  --episode-confidence 74
```

## 5. 让应用使用模型

把导出的 JSON 放到：

```text
%APPDATA%\com.mpvtidy.animesubtitlemanager\parser-crf-model.json
```

重新扫描资源。如果 CRF 参与了判断，解析候选来源里会出现 `crf`。

## 6. 参数怎么理解

- `--min-abs-weight`：导出时丢掉绝对值太小的权重，默认 `0.05`。模型太大可以调高，效果变差就调低。
- `--min-episode-score`：CRF 认为某个 token 是 episode 的最低分数，默认 `2.5`。
- `--min-episode-margin`：episode 分数比其他标签至少高多少才算稳定，默认 `0.8`。
- `--episode-confidence`：CRF 候选进入 Rust parser 后使用的置信度，建议保持 `70-80`，不要高过强规则。
- `--dev-ratio`：留多少样本做验证，默认 `0.2`。

## 7. 结果不好怎么办

优先检查训练数据，而不是先调参数：

- 是否有足够的反例：`1080p`、`10bit`、`H.264`、日期、hash、page、segment。
- 是否同一个作品重复太多，导致模型记住标题。
- 是否 episode 标签太少。
- 是否把错误手动修正也保存进了样本。

模型表现不稳定时，删除 app data 里的 `parser-crf-model.json`，应用会回到 deterministic parser。
