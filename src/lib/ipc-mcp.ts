/**
 * The door the MCP server of the download manager talks through.
 *
 * Off by default. When it is off the bridge refuses every assistant route with
 * a sentence, and the extension's own routes are unaffected.
 */

import { invoke } from "@/lib/ipc";

/** Whether an AI assistant may drive the app. */
export function mcpEnabled(): Promise<boolean> {
  return invoke<boolean>("mcp_enabled");
}

/** Turn the MCP door on or off. */
export function setMcpEnabled(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_mcp_enabled", { enabled });
}
