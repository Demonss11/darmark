// Типизированные обёртки над Tauri IPC (реэкспорт по доменам).

export {
  newDocument,
  openDocument,
  updateDocument,
  renderDocument,
  undoDocument,
  redoDocument,
  saveDocument,
  closeDocument,
  documentSnapshot,
} from "./documents";
export type { DocumentSnapshot, RenderResult } from "./documents";

export {
  pluginViews,
  pluginViewAction,
  listPlugins,
  setPluginEnabled,
  setPluginPermissions,
  setPluginSetting,
  reloadPlugin,
  removePlugin,
  runPluginCommand,
  openDevtools,
} from "./plugins";
export type {
  PluginViewInfo,
  PluginStatus,
  PluginSettingProperty,
  PluginInfo,
} from "./plugins";

export { errorMessage } from "./errors";

export {
  aiGetSettings,
  aiSetSettings,
  aiSetApiKey,
  aiListSkills,
  aiChat,
  aiTestConnection,
  aiRenderMarkdown,
  aiHistoryLoad,
  aiHistorySave,
  aiHistoryClear,
  aiDiff,
  aiApplyEdit,
  aiValidatePlugin,
  aiCreatePlugin,
  aiUpdatePlugin,
  aiDeletePlugin,
} from "./ai";
export type {
  AiSettingsView,
  AiSettingsInput,
  AiMessage,
  AiUsage,
  AiSkill,
  AiDiff,
  ProviderView,
  ProviderInput,
  ModelInput,
} from "./ai";
