use super::*;

/// Сборка снимка: главный — приватный рабочий набор (как в Диспетчере задач), child —
/// private/commit (ADR-0031 §2.1 п.3); лимит Job Object плагина проброшен в
/// `mem_limit_bytes`. Оба процесса получают метрики из **одного** вызова провайдера —
/// класс процесса лишь выбирает поле.
#[test]
fn collect_uses_private_working_set_for_main_and_private_for_child() {
    let provider = FakeProvider::new(vec![
        (
            7,
            RawMetrics {
                working_set_bytes: 300,
                private_bytes: 111,
                private_working_set_bytes: 220,
                cpu_percent: Some(1.5),
                handles: Some(20),
            },
        ),
        (
            42,
            RawMetrics {
                working_set_bytes: 999,
                private_bytes: 250,
                private_working_set_bytes: 500,
                cpu_percent: Some(0.5),
                handles: Some(30),
            },
        ),
    ]);
    let children = vec![("word-count".to_string(), 42)];

    let snap = ResourceSnapshot::collect(&provider, 7, &children, LIMIT, 1_760_000_000_000)
        .expect("главный доступен — снимок есть");

    assert_eq!(snap.ts, 1_760_000_000_000, "ts снимка");
    assert_eq!(
        snap.main.mem_bytes, 220,
        "главный — приватный рабочий набор (как Диспетчер задач)"
    );
    assert_eq!(snap.main.cpu_percent, Some(1.5));
    assert_eq!(snap.main.handles, Some(20));

    assert_eq!(snap.children.len(), 1);
    let child = &snap.children[0];
    assert_eq!(child.plugin_id, "word-count");
    assert_eq!(child.pid, 42);
    assert_eq!(
        child.mem_bytes, 250,
        "child плагина — private/commit, не рабочий набор"
    );
    assert_eq!(child.mem_limit_bytes, LIMIT, "лимит Job Object проброшен");
    assert_eq!(child.cpu_percent, Some(0.5));
    assert_eq!(child.handles, Some(30));
}

/// Фолбэк главного: при `PrivateWorkingSetSize == 0` (старая ОС / неизвестный `cb`,
/// EX2-хвост не заполнен) берётся полный `WorkingSetSize`, а не лживый ноль (F51).
#[test]
fn main_falls_back_to_working_set_when_private_working_set_is_zero() {
    let provider = FakeProvider::new(vec![(
        7,
        RawMetrics {
            working_set_bytes: 300,
            private_bytes: 111,
            private_working_set_bytes: 0,
            cpu_percent: None,
            handles: None,
        },
    )]);

    let snap = ResourceSnapshot::collect(&provider, 7, &[], LIMIT, 1)
        .expect("главный доступен — снимок есть");
    assert_eq!(
        snap.main.mem_bytes, 300,
        "нулевой private WS → фолбэк на полный рабочий набор"
    );
}

/// Недоступный/невалидный PID не попадает в снимок: ни паники, ни записи «0» (ADR-0031
/// §2.3 п.11, F51). Нечитаемый **child** пропускается, остальные дети остаются на месте.
#[test]
fn unreadable_pid_is_skipped_without_zero_record() {
    let provider = FakeProvider::new(vec![(7, raw(100, 0)), (42, raw(10, 20))]);
    let children = vec![
        ("dead".to_string(), 0),
        ("max".to_string(), u32::MAX),
        ("system".to_string(), 4),
        ("alive".to_string(), 42),
    ];

    let snap =
        ResourceSnapshot::collect(&provider, 7, &children, LIMIT, 1).expect("главный 7 доступен");
    assert_eq!(snap.children.len(), 1, "выживает только читаемый PID");
    assert_eq!(snap.children[0].plugin_id, "alive");
    assert_eq!(snap.children[0].pid, 42);
    assert!(
        snap.children.iter().all(|c| c.mem_bytes != 0),
        "недоступная метрика не изображается нулём: {:?}",
        snap.children
    );
}

/// Нечитаемый **главный** процесс → снимка нет целиком (`None`), а не `mem_bytes: 0`
/// (F51/ADR-0031 §2.3 п.11). Даже при читаемых детях отсутствующая метрика главного не
/// изображается нулём: main-процесс практически всегда читаем, ложный ноль недопустим.
#[test]
fn unreadable_main_process_yields_no_snapshot() {
    let provider = FakeProvider::new(vec![(42, raw(10, 20))]);
    assert!(
        ResourceSnapshot::collect(&provider, 7, &[("alive".to_string(), 42)], LIMIT, 1).is_none(),
        "недоступный главный процесс — снимка нет, даже если дети читаются"
    );
    let empty = FakeProvider::new(vec![]);
    assert!(
        ResourceSnapshot::collect(&empty, 4, &[], LIMIT, 1).is_none(),
        "недоступный главный процесс — снимка нет"
    );
}

/// PID наружу выходит только от `Active`-плагина с живым `Supervisor` (`Child::id()`);
/// `Stopped`/`Failed` процессом не владеют и PID не отдают (ADR-0031 §2.6, §2.2 п.7).
#[test]
fn active_pids_returns_only_live_active_children() {
    let _guard = serial();
    let store = store_with_doc("x");
    let mut manager = PluginManager::default();
    let mut runtime = PluginRuntime::new(
        "viewplugin",
        child_exe(),
        fixture("view-activate.lua"),
        vec!["view:create".to_string()],
        services(&store),
        vec![view_contrib("main", "Main", 1)],
        Vec::new(),
    )
    .with_granted(vec!["view:create".to_string()]);
    assert_eq!(runtime.pid(), None, "остановленный плагин PID не отдаёт");
    assert!(manager.active_pids().is_empty());

    runtime.start().expect("активация плагина");
    let pid = runtime.pid().expect("Active-плагин владеет живым child'ом");
    assert!(pid > 0, "реальный PID child'а: {pid}");
    manager.register(runtime);

    assert_eq!(
        manager.active_pids(),
        vec![("viewplugin".to_string(), pid)],
        "активный child попадает в список"
    );
    assert!(
        manager.active_pids().iter().all(|(_, p)| *p != 0),
        "PID 0 не отдаётся наружу"
    );
    manager
        .set_plugin_enabled("viewplugin", false)
        .expect("выключение");
    assert_eq!(manager.get("viewplugin").unwrap().pid(), None);
    assert!(
        manager.active_pids().is_empty(),
        "после остановки список пуст"
    );
    let mut failed = PluginRuntime::new(
        "broken",
        PathBuf::from("darmark-no-such-host.exe"),
        "function on_activate(ctx) end".to_string(),
        Vec::new(),
        services(&store),
        Vec::new(),
        Vec::new(),
    );
    let _ = failed.start();
    assert!(
        matches!(failed.status, PluginStatus::Failed { .. }),
        "неудачный спавн → Failed, статус: {:?}",
        failed.status
    );
    assert_eq!(failed.pid(), None, "Failed не владеет процессом");
    manager.register(failed);
    assert!(
        manager.active_pids().is_empty(),
        "Failed не в списке активных"
    );
}

/// `ResourceSink` получает `resource-metrics` на каждом тике и `plugin-process-exited`,
/// когда child исчез из владения (по образцу тестов sink'ов S11).
#[test]
fn resource_sink_receives_metrics_and_exited_events() {
    let provider = FakeProvider::new(vec![(7, raw(512, 0)), (100, raw(1, 77))]);
    let sink = Arc::new(RecordingSink::default());
    let mut monitor = ResourceMonitor::new(Arc::new(provider), Arc::clone(&sink), 7, LIMIT);

    monitor.tick(&[("wc".to_string(), 100)]);

    let metrics = sink.metrics.lock().unwrap().clone();
    assert_eq!(metrics.len(), 1, "один снимок на тик");
    assert_eq!(
        metrics[0].main.mem_bytes, 512,
        "главный — приватный рабочий набор"
    );
    assert_eq!(
        metrics[0].children[0].mem_bytes, 77,
        "child — private/commit"
    );
    assert!(
        sink.exited.lock().unwrap().is_empty(),
        "живой child не порождает завершение"
    );
    monitor.tick(&[]);
    assert_eq!(
        *sink.exited.lock().unwrap(),
        vec![("wc".to_string(), 100, None)],
        "утрата владения → событие завершения"
    );
}

/// Завершение определяется **владением процессом**, а не пробой «голого PID»: провайдер
/// всё ещё «читает» PID 100 (переиспользован чужим процессом — F52), но менеджер уже не
/// владеет child'ом → `plugin-process-exited` (ADR-0031 §2.4 п.19).
#[test]
fn process_exited_comes_from_ownership_not_pid_probe() {
    let provider = FakeProvider::new(vec![(7, raw(1, 0)), (100, raw(1, 55))]);
    let sink = Arc::new(RecordingSink::default());
    let mut monitor = ResourceMonitor::new(Arc::new(provider), Arc::clone(&sink), 7, LIMIT);

    monitor.tick(&[("wc".to_string(), 100)]);
    monitor.tick(&[]);

    assert_eq!(
        *sink.exited.lock().unwrap(),
        vec![("wc".to_string(), 100, None)],
        "завершение — по утрате владения, не по пробе PID"
    );
}

/// Обратная сторона: недоступная метрика при **сохранённом** владении — это не завершение
/// процесса. Child пропускается в снимке (не «0»), но `plugin-process-exited` не эмитится.
#[test]
fn unreadable_child_with_active_ownership_is_not_an_exit() {
    let provider = FakeProvider::new(vec![(7, raw(1, 0))]);
    let sink = Arc::new(RecordingSink::default());
    let mut monitor = ResourceMonitor::new(Arc::new(provider), Arc::clone(&sink), 7, LIMIT);

    monitor.tick(&[("wc".to_string(), 100)]);
    monitor.tick(&[("wc".to_string(), 100)]);

    assert!(
        sink.exited.lock().unwrap().is_empty(),
        "недоступная метрика ≠ завершение процесса"
    );
    assert!(
        sink.metrics
            .lock()
            .unwrap()
            .iter()
            .all(|s| s.children.is_empty()),
        "нечитаемый child пропущен, а не записан нулём"
    );
}

/// Сериализация снимка в контракт `resource-metrics` (TZ §4.4): имена полей, `mem_limit_bytes`
/// только у детей, `ts`/`pid` — числа.
#[test]
fn snapshot_serializes_frontend_contract() {
    let snap = ResourceSnapshot {
        ts: 1_760_000_000_000,
        main: MainMetrics {
            mem_bytes: 163_577_856,
            cpu_percent: Some(3.1),
            handles: Some(412),
        },
        children: vec![ChildMetrics {
            plugin_id: "word-count".to_string(),
            pid: 12_345,
            mem_bytes: 25_165_824,
            mem_limit_bytes: 67_108_864,
            cpu_percent: Some(0.4),
            handles: Some(96),
        }],
    };

    let value = serde_json::to_value(&snap).unwrap();
    assert_eq!(value["ts"], 1_760_000_000_000u64);
    assert_eq!(value["main"]["mem_bytes"], 163_577_856);
    assert_eq!(value["main"]["cpu_percent"], 3.1);
    assert_eq!(value["main"]["handles"], 412);
    assert!(
        value["main"].get("mem_limit_bytes").is_none(),
        "у главного процесса лимита Job Object нет: {value}"
    );
    assert_eq!(value["children"][0]["plugin_id"], "word-count");
    assert_eq!(value["children"][0]["pid"], 12_345);
    assert_eq!(value["children"][0]["mem_bytes"], 25_165_824);
    assert_eq!(value["children"][0]["mem_limit_bytes"], 67_108_864);
}
