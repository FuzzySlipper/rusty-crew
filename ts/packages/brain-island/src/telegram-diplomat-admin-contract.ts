export const TELEGRAM_DIPLOMAT_ADMIN_CONTRACT_VERSION = "0.1.0";

export const TELEGRAM_DIPLOMAT_ADMIN_OPENAPI_PATH =
  "docs/telegram-diplomat-admin-api-v0.openapi.json";

export const TELEGRAM_DIPLOMAT_ADMIN_PATHS = {
  read: "/v1/admin/telegram-diplomat",
  createBinding: "/v1/admin/telegram-diplomat/bindings",
  binding: "/v1/admin/telegram-diplomat/bindings/{binding_id}",
  moveBinding: "/v1/admin/telegram-diplomat/bindings/{binding_id}/move",
  relabelBinding: "/v1/admin/telegram-diplomat/bindings/{binding_id}/relabel",
  pauseBinding: "/v1/admin/telegram-diplomat/bindings/{binding_id}/pause",
  resumeBinding: "/v1/admin/telegram-diplomat/bindings/{binding_id}/resume",
  removeBinding: "/v1/admin/telegram-diplomat/bindings/{binding_id}/remove",
  credential: "/v1/admin/telegram-diplomat/credential",
  reload: "/v1/admin/telegram-diplomat/reload",
} as const;

export const TELEGRAM_DIPLOMAT_STATE_VALUES = [
  "disabled",
  "unconfigured",
  "disconnected",
  "unbound",
  "ambiguous",
  "rate_limited",
  "healthy",
] as const;
