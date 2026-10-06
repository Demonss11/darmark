# Отчёт: карантин N=3 и граница изоляции (F37/F38) — §3 ADR-0021 закрыт полностью

- **Дата:** 06.10.2026
- **Ветка:** `proto/lua-plugin-host`
- **Повод:** последние два открытых пункта §3 ADR-0021: карантин N=3 и формулировка «изоляция
  отказов, не защита данных» в UI/магазине.
- **Основание:** `docs/adr/0021-plugin-isolation-process.md` §3; `docs/proto/FINDINGS.md` F37/F38.
- **Артефакты:** `crates/lua-rpc-spike/src/lib.rs` (`Quarantine`, `ISOLATION_NOTICE`,
  `DOCUMENT_ACCESS_NOTICE`, `permission_notices`), `src/bin/quarantine_parent.rs`,
  фикстуры `plugins/{crash,hello}.lua`, гейт `crates/lua-rpc-spike/run-d16.ps1`.

> **Статус: §3 ADR-0021 закрыт полностью.** Ранние гейты M10/M11/M12 повторно прогнаны —
> регрессий нет.

---

## 1. F37 — карантин N=3

**Требование (§10.2 DESIGN_DOC, ADR-0021 §3):** после 3 падений подряд плагин автоматически
отключается до ручного включения.

**Решение.** `Quarantine` в `lib.rs` — состояние per-plugin (живёт в хосте, плагин stateless):
`record_failure` копит серию и при пороге выставляет `disabled`; `record_success` сбрасывает
серию; `reset` — ручное включение. Harness `quarantine-parent` симулирует жизненный цикл
супервизора, запуская последовательность фикстур.

**Замеры** (release, Windows/msvc):

| Последовательность | Наблюдение |
|---|---|
| `crash`×4, порог 3 | `failures=1,2,3(disabled=1)` + `QUARANTINE` + `SKIP` — перезапусков нет |
| `crash,crash,hello,crash` | после `hello` `failures=0`; карантин не включён (`disabled=0`) |
| `hello` | `outcome=success failures=0` |

Юнит-тесты: порог/сброс серией/ручное включение.

---

## 2. F38 — формулировка границы изоляции

**Требование (§11.4, ADR-0021 §2.6):** UI и магазин плагинов обязаны явно заявлять, что это
изоляция **отказов**, а не защита данных; плагин с доступом к документу видит его полностью.

**Решение.** Канонические строки в `lib.rs` как единый источник для UI:
- `ISOLATION_NOTICE` — показывается всегда;
- `DOCUMENT_ACCESS_NOTICE` — добавляется функцией `permission_notices(permissions)`, если среди
  разрешений есть `document`/`document:*`.

**Проверка.** `quarantine-parent --permissions document:read` печатает оба `NOTICE`; юнит-тест
`permission_notices_include_isolation_and_document` проверяет состав. Рендеринг в реальном UI —
задача H2; в прототипе закрыт уровень «текст + правило выбора».

---

## 3. Воспроизведение

```powershell
cargo test -p lua-rpc-spike --lib           # 12 юнит-тестов (в т.ч. карантин F37, notices F38)
pwsh -File crates/lua-rpc-spike/run-d16.ps1 # F33 + F35 + F36 + F37 + range/delta, exit 0 = гейт
```

Ранние гейты: `run-m10.ps1`, `run-m11.ps1`, `run-m12.ps1` — прогнаны, `exit=0`.

---

## 4. Итог по §3 ADR-0021

| Пункт | Факт | Статус |
|---|---|---|
| Прогресс-таймаут watchdog | F33 | [x] |
| Абсолютный дедлайн invocation | F35 | [x] |
| Range/delta host-API | F34 | [x] |
| Устойчивость протокола | F36 | [x] |
| Job Object + `KILL_ON_JOB_CLOSE` | F28/F29 | [x] |
| Карантин N=3 | F37 | [x] |
| Граница «изоляция отказов ≠ защита данных» | F38 | [x] |

Открытых пунктов §3 нет. Дальше — H1/H2 по `docs/ROADMAP.md`.
