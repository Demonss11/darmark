//! Тесты tool-loop [`super::super::tool_loop`]: спецификации MCP, вызов, разметка R7, цикл,
//! деградация провайдера без `tools` (ADR-0040).
//!
//! Транспорт провайдера, MCP-серверы и кэш возможностей подменяются двойниками из
//! [`super::support`]: `ScriptedTransport` отдаёт заранее заданную последовательность шагов
//! (включая отказы провайдера), `MockCapability` записывает моменты постановки вердикта. Так
//! проверяется поведение цикла (число запросов, объявление `tools`, состав диалога, деградация),
//! а не реализация сети или реестра.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use mcp_core::protocol::{RpcRequest, RPC_VERSION};
use mcp_core::server::McpServer;

use crate::ai::tool_calls::ToolCall;
use crate::ai::tool_loop::{
    call_mcp_tool, run, tool_message_content, tool_specs_from_tools_list, ToolExecution,
    MAX_TOOL_ITERATIONS, MAX_TOOL_RESULT_CHARS,
};
use crate::ai::transport::ChatRequest;
use crate::ai::types::AiMessage;
use crate::error::{CommandError, ErrorCode};

use super::support::{
    block_on, clone_error, context_length_exceeded, failing_server, fake_server, final_outcome,
    long_text_server, outcome_with_tool_call, tools_rejected, MockCapability, ScriptedTransport,
};

/// Результат `tools/list` из реестра — тот же JSON-RPC путь, что и у провайдера.
fn tools_list_json(server: &McpServer) -> Value {
    server
        .handle(RpcRequest {
            jsonrpc: RPC_VERSION.to_string(),
            id: Some(json!(1)),
            method: "tools/list".to_string(),
            params: None,
        })
        .expect("tools/list возвращает ответ")
        .result
        .expect("tools/list возвращает result")
}

/// Базовый запрос без инструментов: сообщения дополняет `run`.
fn base_request() -> ChatRequest {
    ChatRequest {
        base_url: "https://example.test/v1".to_string(),
        model: "test-model".to_string(),
        api_key: "sk-test".to_string(),
        messages: vec![AiMessage {
            role: "user".to_string(),
            content: "привет".to_string(),
            ..AiMessage::default()
        }],
        max_tokens: 128,
        temperature: 0.0,
        headers: BTreeMap::new(),
        extra_body: serde_json::Map::new(),
        timeout: None,
        tools: None,
    }
}

/// Тот же запрос с объявленным инструментом — вход для проверки деградации: только при
/// непустом `tools` провайдер вообще может отвергнуть поле (ADR-0040 §2.2 п.10).
fn request_with_tools() -> ChatRequest {
    ChatRequest {
        tools: Some(json!([
            {"type": "function", "function": {"name": "fake_tool", "parameters": {}}}
        ])),
        ..base_request()
    }
}

#[test]
fn tool_specs_from_tools_list_maps_mcp_registry() {
    let server = fake_server();
    let list = tools_list_json(&server);

    let specs = tool_specs_from_tools_list(&list);

    assert_eq!(specs.len(), 1, "одна спека на инструмент: {specs:?}");
    assert_eq!(specs[0]["function"]["name"], "fake_tool");
    assert_eq!(specs[0]["type"], "function");
}

#[test]
fn tool_message_content_marks_data_not_instructions() {
    let text = tool_message_content("plugin_validate", "ok");

    assert!(
        text.contains("plugin_validate"),
        "имя инструмента видно: {text}"
    );
    assert!(
        text.to_lowercase().contains("не инструкци"),
        "результат размечается как данные, не инструкции: {text}"
    );
}

#[test]
fn tool_loop_executes_tool_and_returns_final_answer() {
    let server = fake_server();
    let transport = ScriptedTransport::ok(
        vec![outcome_with_tool_call(), final_outcome("готовый ответ")],
        final_outcome("готовый ответ"),
    );
    let mut called: Vec<String> = Vec::new();
    let mut results: Vec<(String, bool)> = Vec::new();

    let response = block_on(run(
        &transport,
        &server,
        base_request(),
        &MockCapability::empty(),
        &mut |_delta: String| {},
        &mut |call: &ToolCall| called.push(call.name.clone()),
        &mut |exec: &ToolExecution| results.push((exec.call.id.clone(), exec.result.ok)),
    ))
    .expect("tool-loop завершается финальным ответом");

    assert_eq!(response.message.role, "assistant");
    assert_eq!(response.message.content, "готовый ответ");
    assert_eq!(called, vec!["fake_tool".to_string()], "инструмент вызван");
    assert_eq!(
        results,
        vec![("call-1".to_string(), true)],
        "колбэк получает id вызова и успешный результат"
    );
    assert_eq!(
        transport.request_count(),
        2,
        "запрос до вызова инструмента и после"
    );

    let requests = transport.requests();
    let tool_messages: Vec<&AiMessage> = requests[1]
        .messages
        .iter()
        .filter(|message| message.role == "tool")
        .collect();
    assert_eq!(tool_messages.len(), 1, "в диалог добавлен tool-результат");
    assert_eq!(
        tool_messages[0].tool_call_id.as_deref(),
        Some("call-1"),
        "tool-сообщение связано с вызовом по id"
    );
}

#[test]
fn tool_loop_stops_at_iteration_limit() {
    let server = fake_server();
    let transport = ScriptedTransport::ok(Vec::new(), outcome_with_tool_call());
    let mut calls = 0usize;

    let response = block_on(run(
        &transport,
        &server,
        base_request(),
        &MockCapability::empty(),
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| calls += 1,
        &mut |_exec: &ToolExecution| {},
    ))
    .expect("исчерпание лимита не роняет цикл");

    assert_eq!(
        transport.request_count(),
        MAX_TOOL_ITERATIONS,
        "не более MAX_TOOL_ITERATIONS запросов"
    );
    assert_eq!(
        calls, MAX_TOOL_ITERATIONS,
        "на каждой итерации инструмент вызывается"
    );
    assert_eq!(response.message.role, "assistant");
}

#[test]
fn tool_loop_isolates_tool_failure() {
    let server = failing_server();
    let transport = ScriptedTransport::ok(
        vec![outcome_with_tool_call(), final_outcome("ответ после сбоя")],
        final_outcome("ответ после сбоя"),
    );
    let mut results: Vec<bool> = Vec::new();

    let response = block_on(run(
        &transport,
        &server,
        base_request(),
        &MockCapability::empty(),
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |exec: &ToolExecution| results.push(exec.result.ok),
    ))
    .expect("сбой инструмента не ломает цикл");

    assert_eq!(
        results,
        vec![false],
        "isError приходит как неуспешный результат, а не ошибка"
    );
    assert_eq!(
        response.message.content, "ответ после сбоя",
        "цикл продолжается до финального ответа"
    );
}

#[test]
fn call_mcp_tool_rejects_unknown_tool() {
    let server = fake_server();

    let error = call_mcp_tool(&server, "ghost", json!({}))
        .expect_err("неизвестный инструмент — ошибка протокола");

    assert_eq!(error.code, ErrorCode::AiBadRequest);
}

#[test]
fn call_mcp_tool_truncates_long_result_on_char_boundary() {
    let server = long_text_server();
    let full = Value::String("я".repeat(MAX_TOOL_RESULT_CHARS + 100)).to_string();

    let result = call_mcp_tool(&server, "long_tool", json!({})).expect("вызов инструмента");

    assert!(result.ok, "успешный вызов: {result:?}");
    assert_eq!(
        result.text.chars().count(),
        MAX_TOOL_RESULT_CHARS,
        "текст обрезан ровно до лимита символов"
    );
    assert!(
        full.starts_with(&result.text),
        "обрезка прошла по границе UTF-8 (многобайтный символ не разорван)"
    );
}

/// Сценарий (а) ADR-0040 §2.2: HTTP 400 при объявленных инструментах даёт ровно один
/// авто-повтор без `tools` в том же `run`; вердикт кэшируется только после успеха.
#[test]
fn run_retries_once_without_tools_on_provider_400() {
    let transport = ScriptedTransport::new(
        vec![
            Err(tools_rejected()),
            Ok(final_outcome("ответ без инструментов")),
        ],
        Err(CommandError::provider(
            500,
            ErrorCode::AiServerError,
            "повтор не должен сверх бюджета",
        )),
    );
    let capability = MockCapability::empty();
    let request = request_with_tools();

    let response = block_on(run(
        &transport,
        &fake_server(),
        request.clone(),
        &capability,
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |_exec: &ToolExecution| {},
    ))
    .expect("один отвергнутый запрос не ломает чат");

    assert_eq!(response.message.content, "ответ без инструментов");
    let requests = transport.requests();
    assert_eq!(requests.len(), 2, "один отвергнутый запрос и один повтор");
    assert!(
        requests[0].tools.is_some(),
        "первый запрос объявляет инструменты: {:?}",
        requests[0].tools
    );
    assert!(
        requests[1].tools.is_none(),
        "повтор уходит без поля tools: {:?}",
        requests[1].tools
    );
    assert_eq!(
        capability.marks(),
        vec![(
            "https://example.test/v1".to_string(),
            "test-model".to_string()
        )],
        "вердикт о неподдержке tools ставится один раз и только после успеха"
    );
}

/// §2.3 п.14: распознанное превышение контекста — не отказ от `tools`, повтор не поможет.
#[test]
fn run_skips_retry_on_context_length_400() {
    let transport = ScriptedTransport::new(
        vec![Err(context_length_exceeded())],
        Err(context_length_exceeded()),
    );
    let capability = MockCapability::empty();

    let error = block_on(run(
        &transport,
        &fake_server(),
        request_with_tools(),
        &capability,
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |_exec: &ToolExecution| {},
    ))
    .expect_err("превышение контекста пользователю не чинится повтором");

    assert_eq!(error.code, ErrorCode::AiContextLength);
    assert_eq!(error.http_status, Some(400));
    assert_eq!(
        transport.request_count(),
        1,
        "повтор без tools не выполняется"
    );
    assert!(
        capability.marks().is_empty(),
        "вердикт о tools не ставится: {:?}",
        capability.marks()
    );
}

/// §2.2 п.6 / §2.3 п.15: повтор — только на 400 провайдера **и** только при объявленных
/// инструментах. Таблица прогоняет каждую отказную ветку в обоих режимах: `401`, `5xx`,
/// локальная ошибка без статуса и сам `400` при пустом реестре MCP — все уходят пользователю
/// как есть, без повтора и без вердикта.
#[test]
fn run_does_not_retry_on_other_failures() {
    let cases = [
        (
            "400 — отказ шлюза, но инструменты не объявлялись",
            tools_rejected(),
            false,
        ),
        (
            "401 — отказ ключа",
            CommandError::provider(401, ErrorCode::AiBadKey, "Неверный API-ключ"),
            true,
        ),
        (
            "401 — отказ ключа, но инструменты не объявлялись",
            CommandError::provider(401, ErrorCode::AiBadKey, "Неверный API-ключ"),
            false,
        ),
        (
            "503 — транзиентная ошибка",
            CommandError::provider(503, ErrorCode::AiServerError, "провайдер недоступен"),
            true,
        ),
        (
            "локальная ошибка без статуса",
            CommandError::new(ErrorCode::AiBadRequest, "ответ превысил допустимый размер"),
            true,
        ),
        (
            "локальная ошибка без статуса, инструменты не объявлялись",
            CommandError::new(ErrorCode::AiBadRequest, "ответ превысил допустимый размер"),
            false,
        ),
    ];

    for (label, failure, tools_declared) in cases {
        let transport =
            ScriptedTransport::new(vec![Err(clone_error(&failure))], Ok(final_outcome("x")));
        let capability = MockCapability::empty();
        let request = if tools_declared {
            request_with_tools()
        } else {
            base_request()
        };

        let error = block_on(run(
            &transport,
            &fake_server(),
            request,
            &capability,
            &mut |_delta: String| {},
            &mut |_call: &ToolCall| {},
            &mut |_exec: &ToolExecution| {},
        ))
        .unwrap_err();

        assert_eq!(
            error.message, failure.message,
            "наружу уходит исходная ошибка для {label}"
        );
        assert_eq!(
            error.http_status, failure.http_status,
            "статус не подменяется для {label}"
        );
        assert_eq!(
            transport.request_count(),
            1,
            "повтор не выполняется для {label}"
        );
        assert!(
            capability.marks().is_empty(),
            "вердикт не ставится для {label}: {:?}",
            capability.marks()
        );
    }
}

/// §2.2 п.10: деградировать нечего — при пустом реестре MCP в запросе нет `tools`. Регрессия на
/// инверсию в `declares_tools`: отказ `400` без объявленных инструментов обязан уйти наружу как
/// есть, одним запросом и без ложного вердикта «провайдер не умеет `tools`».
#[test]
fn run_does_not_degrade_when_tools_never_declared() {
    let variants = [("tools = None", None), ("tools = []", Some(json!([])))];

    for (label, tools) in variants {
        let transport = ScriptedTransport::new(
            vec![Err(tools_rejected())],
            Err(CommandError::provider(
                400,
                ErrorCode::AiBadRequest,
                "лишний повтор без объявленных инструментов",
            )),
        );
        let capability = MockCapability::empty();

        let error = block_on(run(
            &transport,
            &fake_server(),
            ChatRequest {
                tools,
                ..base_request()
            },
            &capability,
            &mut |_delta: String| {},
            &mut |_call: &ToolCall| {},
            &mut |_exec: &ToolExecution| {},
        ))
        .expect_err("отказ без объявленных инструментов уходит пользователю");

        assert_eq!(
            error.message, "tools is not supported",
            "наружу уходит исходная ошибка для {label}"
        );
        assert_eq!(error.http_status, Some(400));
        assert_eq!(
            transport.request_count(),
            1,
            "повтор идентичен первому запросу и не выполняется для {label}"
        );
        assert!(
            capability.marks().is_empty(),
            "ложный вердикт «не умеет tools» не ставится для {label}: {:?}",
            capability.marks()
        );
    }
}

/// §2.5 п.22: 400 — вердикт по одному ответу. Упавший повтор не кэшируется — пользователь видит
/// исходную ошибку, следующий запрос проверяет заново.
#[test]
fn run_does_not_cache_verdict_when_retry_also_fails() {
    let second = CommandError::provider(
        400,
        ErrorCode::AiBadRequest,
        "tools is not supported (retry)",
    );
    let transport = ScriptedTransport::new(
        vec![Err(tools_rejected()), Err(clone_error(&second))],
        Err(tools_rejected()),
    );
    let capability = MockCapability::empty();

    let error = block_on(run(
        &transport,
        &fake_server(),
        request_with_tools(),
        &capability,
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |_exec: &ToolExecution| {},
    ))
    .expect_err("упавший повтор уходит пользователю");

    assert_eq!(
        error.message, "tools is not supported (retry)",
        "наружу уходит ошибка второго запроса, а не первого"
    );
    assert_eq!(
        transport.request_count(),
        2,
        "после упавшего повтора попыток нет"
    );
    assert!(
        capability.marks().is_empty(),
        "недоказанный вердикт не кэшируется: {:?}",
        capability.marks()
    );
}

/// §2.5 п.25: кэш-хит снимает `tools` до первого запроса — ни одного отвергнутого запроса.
#[test]
fn run_drops_tools_before_first_request_on_cache_hit() {
    let transport = ScriptedTransport::ok(Vec::new(), final_outcome("ответ без инструментов"));
    let capability = MockCapability::with_unsupported("https://example.test/v1", "test-model");

    let response = block_on(run(
        &transport,
        &fake_server(),
        request_with_tools(),
        &capability,
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |_exec: &ToolExecution| {},
    ))
    .expect("кэш-хит не ломает чат");

    assert_eq!(response.message.content, "ответ без инструментов");
    let requests = transport.requests();
    assert_eq!(requests.len(), 1, "один запрос без повтора");
    assert!(
        requests[0].tools.is_none(),
        "кэш-хит снимает tools до первого запроса: {:?}",
        requests[0].tools
    );
    assert!(
        capability.marks().is_empty(),
        "кэш-хит молчит: повторов нет, вердиктов нет"
    );
}

/// §2.2 п.8: бюджет запросов не растёт — повтор занимает слот итерации, а не добавляет новый.
#[test]
fn run_request_budget_includes_degradation_attempt() {
    let transport = ScriptedTransport::new(
        vec![
            Err(tools_rejected()),
            Ok(outcome_with_tool_call()),
            Ok(final_outcome("ответ после вызова")),
        ],
        Ok(final_outcome("ответ после вызова")),
    );
    let capability = MockCapability::empty();

    let response = block_on(run(
        &transport,
        &fake_server(),
        request_with_tools(),
        &capability,
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |_exec: &ToolExecution| {},
    ))
    .expect("цикл доходит до финального ответа");

    assert_eq!(response.message.content, "ответ после вызова");
    assert!(
        transport.request_count() <= MAX_TOOL_ITERATIONS,
        "деградация не увеличила бюджет запросов: {}",
        transport.request_count()
    );
    assert_eq!(capability.marks().len(), 1, "вердикт один");
}

/// §2.2 п.8: повтор занимает слот итерации, поэтому на **последнем** слоте повтор уже некуда
/// делать. Отказ `400` там — исходная ошибка наружу, без «повтора» за пределами бюджета и без
/// вердикта в кэше.
#[test]
fn run_reports_error_when_rejection_hits_last_slot() {
    let transport = ScriptedTransport::new(
        vec![
            Ok(outcome_with_tool_call()),
            Ok(outcome_with_tool_call()),
            Ok(outcome_with_tool_call()),
            Err(tools_rejected()),
        ],
        Err(CommandError::provider(
            400,
            ErrorCode::AiBadRequest,
            "запрос сверх бюджета итераций",
        )),
    );
    let capability = MockCapability::empty();

    let error = block_on(run(
        &transport,
        &fake_server(),
        request_with_tools(),
        &capability,
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |_exec: &ToolExecution| {},
    ))
    .expect_err("повтор на последнем слоте невозможен — ошибка уходит наружу");

    assert_eq!(error.http_status, Some(400));
    assert_eq!(error.message, "tools is not supported");
    assert_eq!(
        transport.request_count(),
        MAX_TOOL_ITERATIONS,
        "бюджет запросов не превышен"
    );
    assert!(
        capability.marks().is_empty(),
        "незавершённая деградация не кэшируется: {:?}",
        capability.marks()
    );
}

/// Обратная граница: отказ на **предпоследнем** слоте — единственный слот, где повтор ещё
/// помещается в бюджет, и он должен сработать.
#[test]
fn run_retries_when_rejection_hits_penultimate_slot() {
    let transport = ScriptedTransport::new(
        vec![
            Ok(outcome_with_tool_call()),
            Ok(outcome_with_tool_call()),
            Err(tools_rejected()),
            Ok(final_outcome("ответ после деградации")),
        ],
        Ok(final_outcome("запасной ответ")),
    );
    let capability = MockCapability::empty();

    let response = block_on(run(
        &transport,
        &fake_server(),
        request_with_tools(),
        &capability,
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |_exec: &ToolExecution| {},
    ))
    .expect("повтор на предпоследнем слоте помещается в бюджет");

    assert_eq!(response.message.content, "ответ после деградации");
    let requests = transport.requests();
    assert_eq!(requests.len(), MAX_TOOL_ITERATIONS);
    assert!(
        requests[requests.len() - 1].tools.is_none(),
        "повтор уходит без tools: {:?}",
        requests[requests.len() - 1].tools
    );
    assert_eq!(
        capability.marks(),
        vec![(
            "https://example.test/v1".to_string(),
            "test-model".to_string()
        )],
        "успешный повтор на последнем слоте кэширует вердикт"
    );
}

/// Сценарий (б) ADR-0040 §2.4: 200 с пустыми `tool_calls` — штатный финальный ответ цикла.
/// Повтора нет: первый запрос уже отправил дельты под тем же `message_id`.
#[test]
fn run_returns_final_answer_on_empty_tool_calls_without_retry() {
    let transport = ScriptedTransport::ok(Vec::new(), final_outcome("обычный ответ"));
    let capability = MockCapability::empty();

    let response = block_on(run(
        &transport,
        &fake_server(),
        request_with_tools(),
        &capability,
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |_exec: &ToolExecution| {},
    ))
    .expect("пустые tool_calls — обычный ответ");

    assert_eq!(response.message.content, "обычный ответ");
    assert_eq!(
        transport.request_count(),
        1,
        "сценарий (б) не порождает повтор"
    );
    assert!(
        capability.marks().is_empty(),
        "успешный ответ не считается отказом от tools: {:?}",
        capability.marks()
    );
}

/// Регрессия: обычный успешный ход не оставляет вердикта в кэше — провайдер остаётся «умеющим»
/// `tools`, иначе деградация была бы залипающей.
#[test]
fn run_keeps_capability_untouched_on_success() {
    let transport = ScriptedTransport::ok(
        vec![outcome_with_tool_call(), final_outcome("готовый ответ")],
        final_outcome("готовый ответ"),
    );
    let capability = MockCapability::empty();

    block_on(run(
        &transport,
        &fake_server(),
        request_with_tools(),
        &capability,
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |_exec: &ToolExecution| {},
    ))
    .expect("цикл завершается ответом");

    assert!(
        capability.marks().is_empty(),
        "успешный первый запрос не ставит вердикт: {:?}",
        capability.marks()
    );
    let requests = transport.requests();
    assert!(
        requests.iter().all(|request| request.tools.is_some()),
        "инструменты продолжают объявляться: {:?}",
        requests
            .iter()
            .map(|request| request.tools.is_some())
            .collect::<Vec<_>>()
    );
}

/// Проверка типов: двойники тестов обязаны оставаться `Send + Sync` (ADR-0040 §2.7 п.31).
#[test]
fn mock_capability_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MockCapability>();
    assert_send_sync::<ScriptedTransport>();
}
