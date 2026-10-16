//! Тесты harness'а плагинов (ADR-0034 §2.4, TZ-MCP R3/AC-MCP-12): безопасный id и манифест,
//! белый список каталогов, ядро CRUD и запуск сайдкара с таймаутом.

mod common;
mod crud;
mod mcp;
mod mcp_read;
mod paths;
mod proc;
mod validate;
