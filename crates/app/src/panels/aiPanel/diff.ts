

import { buildUnitMaps, bytesToUnits } from "../../core/mapping";
import type { AiDiff } from "../../core/tauri";
import { el } from "./dom";

const MAX_DIFF_LINES = 500;

function splitLines(text: string): string[] {
  if (text === "") return [];
  const lines = text.split("\n");
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

function appendDiffLine(pre: HTMLElement, kind: "add" | "del", line: string): void {
  const span = el("span", kind === "add" ? "ai-diff-add" : "ai-diff-del");
  span.textContent = `${kind === "add" ? "+" : "-"} ${line}`;
  pre.appendChild(span);
}

export function renderDiffBody(pre: HTMLElement, diff: AiDiff): void {
  const maps = buildUnitMaps(diff.original);
  const ops = [...diff.ops].sort((a, b) => a.start - b.start);
  let shown = 0;
  let truncated = false;
  for (const op of ops) {
    const removed = diff.original.slice(bytesToUnits(maps, op.start), bytesToUnits(maps, op.end));
    for (const line of splitLines(removed)) {
      if (shown >= MAX_DIFF_LINES) {
        truncated = true;
        break;
      }
      appendDiffLine(pre, "del", line);
      shown++;
    }
    if (truncated) break;
    for (const line of splitLines(op.text)) {
      if (shown >= MAX_DIFF_LINES) {
        truncated = true;
        break;
      }
      appendDiffLine(pre, "add", line);
      shown++;
    }
    if (truncated) break;
  }
  if (truncated) {
    const note = el("span", "ai-diff-more");
    note.textContent = `… дифф сокращён (лимит ${MAX_DIFF_LINES} строк)`;
    pre.appendChild(note);
  }
}
