// clean-tauri.mjs — удаляет артефакты сборки Tauri, которые НЕ трогает `cargo clean`.
//
// Зачем: `cargo clean` чистит только `target/`. Всё, что Tauri/Vite кладёт рядом
// с исходниками, остаётся лежать после clean и незаметно протухает:
//   src-tauri/gen/       — ACL-схемы (пересоздаёт tauri-build при компиляции)
//   src-tauri/binaries/  — копии сайдкаров (остаются, даже когда target/ давно удалён)
//   dist/                — выгрузка фронта для tauri build
//
// target/ намеренно НЕ трогаем: его чистит `cargo clean`, а полная пересборка
// крейтов стоит минут пять.
//
// ВАЖНО: после удаления src-tauri/binaries/ команды `cargo check -p darmark` и
// `cargo test -p darmark` будут падать — tauri-build копирует externalBin рядом с
// бинарником и требует, чтобы сайдкары уже существовали. Перед ними выполните
//   cd crates/app && npm run sidecars      (release, как для tauri build)
//   cd crates/app && npm run sidecars:dev  (debug, как для tauri dev)
//
// Запуск:
//   cd crates/app && npm run clean:tauri

import { existsSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url)); // crates/app/scripts
const appDir = resolve(scriptDir, ".."); // crates/app

const targets = [
  { path: join(appDir, "src-tauri", "gen"), label: "ACL-схемы Tauri" },
  { path: join(appDir, "src-tauri", "binaries"), label: "сайдкары (копии)" },
  { path: join(appDir, "dist"), label: "выгрузка фронта" },
];

let removed = 0;
for (const { path, label } of targets) {
  // Каталога может не быть (например, dist до первого `npm run build`) —
  // это не ошибка, повторный запуск должен быть безопасным.
  if (!existsSync(path)) {
    console.log(`[clean] пропущено (нет): ${label}`);
    continue;
  }
  rmSync(path, { recursive: true, force: true });
  removed += 1;
  console.log(`[clean] удалено: ${label} → ${path}`);
}

if (removed === 0) console.log("[clean] чистить было нечего");

console.log("[clean] target/ не тронут — при необходимости `cargo clean`");
if (removed > 0) {
  console.log("[clean] перед `cargo check -p darmark` выполните `npm run sidecars`");
}