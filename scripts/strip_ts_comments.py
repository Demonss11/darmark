#!/usr/bin/env python3
"""Удаляет все комментарии из TypeScript-файлов (включая JSDoc `/** ... */`).

Однострочные `//`, блочные `/* ... */` и JSDoc `/** ... */` комментарии удаляются.

Обработка корректна для строк ('...', "..."), шаблонных строк (`...`) и
регулярных выражений (/pattern/flags): `//` и `/*` внутри них комментариями
не считаются. Для различения деления и регулярного выражения используется
эвристика по предшествующему токену.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# Символы после которых `/` скорее всего начинает регулярное выражение, а не деление.
_REGEX_PRECEDING_CHARS = set("=(:,[{!&|?{};+-*%^~<>\n")

# Слова после которых `/` — регулярное выражение.
_REGEX_PRECEDING_WORDS = {
    "return", "case", "in", "of", "typeof", "instanceof", "new", "delete",
    "void", "throw", "do", "else", "yield", "await",
}


def strip_comments(text: str) -> str:
    """Возвращает текст без комментариев (включая JSDoc)."""
    out: list[str] = []
    i = 0
    n = len(text)
    prev_significant = ""
    prev_word = ""
    prev_operand = False

    while i < n:
        ch = text[i]
        nxt = text[i + 1] if i + 1 < n else ""

        # --- Блочный комментарий: /* ... */ (включая JSDoc /** ... */) ---
        if ch == "/" and nxt == "*":
            end = text.find("*/", i + 2)
            end = n if end == -1 else end + 2
            if end < n and text[end] == "\n":
                out.append("\n")
            i = end
            prev_significant = ""
            prev_word = ""
            prev_operand = False
            continue

        # --- Однострочный комментарий: // ---
        if ch == "/" and nxt == "/":
            end = text.find("\n", i)
            i = n if end == -1 else end
            prev_word = ""
            continue

        # --- Регулярное выражение: /pattern/flags ---
        if ch == "/" and _is_regex_context(prev_significant, prev_word, prev_operand):
            j = i + 1
            in_class = False
            found_end = False
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == "[":
                    in_class = True
                elif text[j] == "]":
                    in_class = False
                elif text[j] == "/" and not in_class:
                    j += 1
                    while j < n and text[j].isalpha():
                        j += 1
                    found_end = True
                    break
                elif text[j] == "\n":
                    break
                else:
                    j += 1
            if found_end:
                out.append(text[i:j])
                i = j
                prev_significant = "/"
                prev_word = ""
                prev_operand = False
                continue
            # Не нашли закрывающий / до перевода строки — это деление.

        # --- Строка: '...' или "..." ---
        if ch in ("'", '"'):
            quote = ch
            j = i + 1
            found_end = False
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == quote:
                    j += 1
                    found_end = True
                    break
                if text[j] == "\n":
                    break
                j += 1
            if found_end:
                out.append(text[i:j])
                i = j
                prev_significant = quote
                prev_word = ""
                prev_operand = True
                continue

        # --- Шаблонная строка: `...` ---
        if ch == "`":
            j = i + 1
            depth = 0
            found_end = False
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == "$" and j + 1 < n and text[j + 1] == "{":
                    depth += 1
                    j += 2
                    continue
                if depth > 0 and text[j] == "}":
                    depth -= 1
                    j += 1
                    continue
                if depth == 0 and text[j] == "`":
                    j += 1
                    found_end = True
                    break
                j += 1
            if found_end:
                out.append(text[i:j])
                i = j
                prev_significant = "`"
                prev_word = ""
                prev_operand = True
                continue

        # --- Обычный символ ---
        out.append(ch)
        if ch.isalnum() or ch == "_":
            prev_word += ch
        else:
            if ch in (")", "]", "}"):
                prev_operand = True
            elif ch not in " \t\r\n":
                prev_significant = ch
                prev_operand = False
            else:
                # Пробел: prev_word был идентификатором — значит предыдущий токен операнд.
                if prev_word:
                    prev_operand = True
                if ch in "\r\n":
                    prev_significant = ""
            prev_word = ""
        i += 1

    return "".join(out)


def _is_regex_context(prev_significant: str, prev_word: str, prev_operand: bool) -> bool:
    """Эвристика: ли `/` началом регулярного выражения."""
    if prev_operand:
        return False
    if prev_word in _REGEX_PRECEDING_WORDS:
        return True
    if not prev_significant:
        return True
    return prev_significant in _REGEX_PRECEDING_CHARS


def process_file(path: Path, in_place: bool) -> tuple[int, int]:
    """Обработать файл. Возвращает (строк_было, строк_стало)."""
    original = path.read_text(encoding="utf-8")
    cleaned = strip_comments(original)
    lines_before = original.count("\n") + (0 if original.endswith("\n") or not original else 1)
    lines_after = cleaned.count("\n") + (0 if cleaned.endswith("\n") or not cleaned else 1)
    if cleaned != original:
        if in_place:
            path.write_text(cleaned, encoding="utf-8")
        else:
            sys.stdout.write(cleaned)
    return lines_before, lines_after


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Удаляет все комментарии из TypeScript-файлов."
    )
    parser.add_argument("paths", nargs="+", type=Path, help="Файлы или каталоги для обработки.")
    parser.add_argument(
        "-i", "--in-place", action="store_true",
        help="Перезаписывать файлы; без флага результат выводится в stdout.",
    )
    args = parser.parse_args(argv)

    files: list[Path] = []
    for raw in args.paths:
        if raw.is_dir():
            files.extend(sorted(raw.rglob("*.ts")))
            files.extend(sorted(raw.rglob("*.tsx")))
        else:
            files.append(raw)

    if not files:
        print("Не найдено .ts/.tsx файлов.", file=sys.stderr)
        return 1

    total_before = total_after = 0
    for path in files:
        before, after = process_file(path, args.in_place)
        total_before += before
        total_after += after
        if args.in_place:
            print(f"{path}: {before} -> {after} строк")

    if args.in_place:
        print(f"Всего: {total_before} -> {total_after} строк.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
