//! The door the MCP server of the download manager talks through.
//!
//! The server is a separate program for an assistant to start; this switch
//! decides whether it may reach this application at all. Off by default: the
//! bridge refuses every assistant route while it is off.

use tauri::State;

use crate::state::AppState;

/// Whether an AI assistant may drive the app through the MCP door.
#[tauri::command]
pub fn mcp_enabled(state: State<'_, AppState>) -> bool {
    state.engine_settings().mcp_enabled
}

/// Turn the MCP door on or off, and keep the choice.
#[tauri::command]
pub fn set_mcp_enabled(state: State<'_, AppState>, enabled: bool) -> bool {
    state.modify_settings(|settings| {
        settings.mcp_enabled = enabled;
    });
    state.save_engine_settings();
    enabled
}
