#!/usr/bin/env python3
"""Удаляет комментарии из Rust-файлов, сохраняя внутренние док-комментарии.

По умолчанию оставляются только `//!` (строчные) и `/*! ... */` (блочные),
которые документируют содержащий их модуль/элемент. Строчные `//` и блочные
`/* ... */` комментарии вместе с содержимым удаляются; символ перевода строки
после строчного комментария сохраняется.

Флаг --keep-outer-doc дополнительно сохраняет `///` и `/** ... */` — они нужны
там, где док-комментарии несут контракты (ADR-0039).

Обработка корректна для строк, raw-строк, литералов символов и времён жизни:
`//` и `/*` внутри них комментариями не считаются. Блочные комментарии
обрабатываются с учётом вложенности.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def strip_comments(text: str, keep_outer_doc: bool) -> str:
    """Возвращает текст без комментарии, кроме `//!`/`/*!` (и `///` при флаге)."""
    out: list[str] = []
    i = 0
    n = len(text)
    # Состояние блочного комментария: глубина вложенности и признак док-комментария.
    block_depth = 0
    block_is_doc = False

    while i < n:
        ch = text[i]
        nxt = text[i + 1] if i + 1 < n else ""

        # --- Внутри блочного комментария ---
        if block_depth:
            if ch == "/" and nxt == "*":
                block_depth += 1
                i += 2
                continue
            if ch == "*" and nxt == "/":
                block_depth -= 1
                if block_depth == 0 and block_is_doc:
                    # Закрывающий `*/` док-блока — сохраняем его.
                    out.append(text[block_start:i + 2])
                if block_depth == 0:
                    block_is_doc = False
                i += 2
                continue
            i += 1
            continue

        # --- Строковые литералы ---
        if ch == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            out.append(text[i:j])
            i = j
            continue

        # --- Строковые литералы с префиксами: r, r#, b, br, c, cr и их комбинации ---
        if ch in "rbc":
            m = _match_prefixed_string(text, i)
            if m is not None:
                out.append(text[i:m])
                i = m
                continue

        # --- Литерал символа (не время жизни) ---
        if ch == "'":
            end = _match_char_literal(text, i)
            if end is not None:
                out.append(text[i:end])
                i = end
                continue
        # Иначе `'a` — время жизни, просто символ.

        # --- Комментарии ---
        if ch == "/" and nxt == "/":
            # `//!` — строчный док-комментарий (не `//!/`).
            is_inner_doc = i + 2 < n and text[i + 2] == "!" and not (
                i + 3 < n and text[i + 3] == "/"
            )
            # `///` — внешний док-комментарий (не `////` и не `///!`).
            is_outer_doc = (
                not is_inner_doc
                and i + 2 < n
                and text[i + 2] == "/"
                and not (
                    i + 3 < n
                    and (text[i + 3] == "/" or text[i + 3] == "!")
                )
            )
            if is_inner_doc or (is_outer_doc and keep_outer_doc):
                end = text.find("\n", i)
                end = n if end == -1 else end
                out.append(text[i:end])
                i = end
                continue
            # Обычный строчный комментарий: удаляем до перевода строки.
            end = text.find("\n", i)
            i = n if end == -1 else end
            continue

        if ch == "/" and nxt == "*":
            # `/*!` — блочный док-комментарий (не `/*!/`).
            is_inner_doc = i + 2 < n and text[i + 2] == "!" and not (
                i + 3 < n and text[i + 3] == "/"
            )
            # `/**` — внешний блочный док-комментарий (не `/***` и не `/**/`).
            is_outer_doc = (
                not is_inner_doc
                and i + 2 < n
                and text[i + 2] == "*"
                and not (
                    i + 3 < n
                    and (text[i + 3] == "*" or text[i + 3] == "/")
                )
            )
            if is_inner_doc or (is_outer_doc and keep_outer_doc):
                block_depth = 1
                block_is_doc = True
                block_start = i
                i += 2
                continue
            block_depth = 1
            block_is_doc = False
            i += 2
            continue

        out.append(ch)
        i += 1

    return "".join(out)


def _match_prefixed_string(text: str, i: int) -> int | None:
    """Если с позиции i начинается префиксная строка (r/b/c и #), вернуть конец."""
    n = len(text)
    j = i
    while j < n and text[j] in "rbc":
        j += 1
    if j == i or j >= n:
        return None
    # Хотя бы один `#` перед кавычкой.
    hash_start = j
    while j < n and text[j] == "#":
        j += 1
    if j == hash_start or j >= n or text[j] != '"':
        return None
    hashes = j - hash_start
    j += 1  # открывающая кавычка
    while j < n:
        if text[j] == "\\":
            j += 2
            continue
        if text[j] == '"':
            j += 1
            # Закрывающие `#` в том же количестве — иначе это не конец строки.
            k = 0
            while k < hashes and j < n and text[j] == "#":
                j += 1
                k += 1
            if k == hashes:
                return j
            # Иначе `"` внутри строки — продолжаем поиск.
            continue
        j += 1
    return j  # Незакрытая строка — конец файла.


def _match_char_literal(text: str, i: int) -> int | None:
    r"""Если с позиции i начинается литерал символа, вернуть конец; иначе None.

    Литерал символа: `'x'`, `'\n'`, `'\u{1F600}'`, плюс префикс `b` (b'a').
    Не является литералом — время жизни вроде `'a`.
    """
    n = len(text)
    j = i
    if j < n and text[j] == "b":
        j += 1
    if j >= n or text[j] != "'":
        return None
    j += 1  # открывающая кавычка
    if j < n and text[j] == "\\":
        j += 1
        if j >= n:
            return None
        j += 1
        # Экранирование вида \u{...}
        if text[j - 1] == "u" and j < n and text[j] == "{":
            close = text.find("}", j)
            if close == -1:
                return None
            j = close + 1
    else:
        if j >= n:
            return None
        j += 1
    if j >= n or text[j] != "'":
        return None  # Нет закрывающей кавычки — это время жизни.
    return j + 1


def process_file(path: Path, keep_outer_doc: bool, in_place: bool) -> tuple[int, int]:
    """Обработать файл. Возвращает (строк_было, строк_стало)."""
    original = path.read_text(encoding="utf-8")
    cleaned = strip_comments(original, keep_outer_doc)
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
        description="Удаляет комментарии из Rust-файлов, сохраняя `//!` и `/*!`."
    )
    parser.add_argument("paths", nargs="+", type=Path, help="Файлы или каталоги для обработки.")
    parser.add_argument(
        "--keep-outer-doc",
        action="store_true",
        help="Дополнительно сохранять внешние док-комментарии `///` и `/** ... */`.",
    )
    parser.add_argument(
        "-i",
        "--in-place",
        action="store_true",
        help="Перезаписывать файлы; без флага результат выводится в stdout.",
    )
    args = parser.parse_args(argv)

    files: list[Path] = []
    for raw in args.paths:
        if raw.is_dir():
            files.extend(sorted(raw.rglob("*.rs")))
        else:
            files.append(raw)

    if not files:
        print("Не найдено .rs файлов.", file=sys.stderr)
        return 1

    total_before = total_after = 0
    for path in files:
        before, after = process_file(path, args.keep_outer_doc, args.in_place)
        total_before += before
        total_after += after
        if args.in_place:
            print(f"{path}: {before} -> {after} строк")

    if args.in_place:
        print(f"Всего: {total_before} -> {total_after} строк.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
