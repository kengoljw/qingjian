#!/usr/bin/env python3
"""从 base/p2c/ext 三份 `--eval-details` 明细里算同一套指标，并做配对 McNemar 精确检验。

三份文件是按「同一批候选」记录的，但顺序可能不同（p2c 是引擎自己重排的，ext 是模型离线
重排的），所以指标一律**从 `candidates` 现算**，不读记录里的 `position`/`top`：
`position` 只在原始顺序下有意义。
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path


def load(path: str):
    """逐句读出 (gold, top1, correct_at_3, char_accuracy, query_ms)。"""
    rows = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            row = json.loads(line)
            gold = row.get("text", "")
            candidates = row.get("candidates") or []
            if not candidates:
                rows.append((gold, None, False, 0.0, row.get("query_ms")))
                continue
            position = next((i for i, c in enumerate(candidates) if c == gold), None)
            # 与 CLI 同一口径：第一个字数等于原句的候选就是整句答案
            length = len(gold)
            sentence = next((c for c in candidates if len(c) == length), None)
            if sentence is None:
                char_acc = 0.0
            else:
                same = sum(1 for a, b in zip(sentence, gold) if a == b)
                char_acc = same / max(length, 1)
            # ext 记录里的 `query_ms` 是从 base 原样带过来的，不代表重排耗时；
            # 有 `rerank_ms`（ime-lm-rerank 写的）就用它，否则退回 `query_ms`。
            rows.append(
                (
                    gold,
                    candidates[0],
                    position is not None and position < 3,
                    char_acc,
                    row.get("rerank_ms", row.get("query_ms")),
                )
            )
    return rows


def mcnemar(a, b):
    """配对 McNemar 精确检验：a/b 是逐句的布尔正确性。返回 (b01, b10, p)。"""
    b01 = sum(1 for x, y in zip(a, b) if not x and y)
    b10 = sum(1 for x, y in zip(a, b) if x and not y)
    n = b01 + b10
    if n == 0:
        return b01, b10, 1.0
    k = min(b01, b10)
    tail = sum(math.comb(n, i) for i in range(0, k + 1)) / (2**n)
    return b01, b10, min(1.0, 2 * tail)


def percentile(values, p):
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, int(round((len(ordered) - 1) * p)))
    return ordered[index]


def summarize(name: str, rows):
    total = len(rows)
    top1 = sum(1 for r in rows if r[1] == r[0])
    top3 = sum(1 for r in rows if r[2])
    chars = sum(r[3] for r in rows)
    times = [r[4] for r in rows if isinstance(r[4], (int, float))]
    print(
        f"{name:<10} n={total:<6} 首选 {top1 / total * 100:5.2f}%  "
        f"前三 {top3 / total * 100:5.2f}%  字准 {chars / total * 100:5.2f}%"
    )
    if times:
        print(
            f"{'':<10} 耗时 p50 {percentile(times, 0.5):7.1f}  p95 {percentile(times, 0.95):7.1f}  "
            f"最慢 {max(times):8.1f} ms"
        )
    return top1, top3, chars / total * 100


def main() -> None:
    files = {name: load(path) for name, path in (a.split("=", 1) for a in sys.argv[1:])}
    print("== 汇总（同池，逐句现算） ==\n")
    stats = {name: summarize(name, rows) for name, rows in files.items()}

    base = files.get("base")
    if base is None:
        return
    print("\n== 配对 McNemar 精确检验（对 base，首选正确与否） ==\n")
    base_top1 = [r[1] == r[0] for r in base]
    for name, rows in files.items():
        if name == "base":
            continue
        other = [r[1] == r[0] for r in rows]
        b01, b10, p = mcnemar(base_top1, other)
        print(
            f"{name:<10} 翻好 {b01:<5} 翻坏 {b10:<5} 净 {b01 - b10:+5}  "
            f"McNemar p = {p:.4g}"
        )


if __name__ == "__main__":
    main()
