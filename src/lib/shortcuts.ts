/**
 * The keyboard shortcut registry.
 *
 * Every key the application listens for lives here as data: a command id, a
 * default combination, and the scope that decides where the key is heard. The
 * reader's changes are stored as overrides in `localStorage`, beside the other
 * saved preferences, so a rebound key survives a restart and every window
 * reads one copy.
 *
 * The registry is three modules behind this path: the shipped data, the
 * reading and printing of one combination, and the reader's overrides. The
 * Settings page, the menus, the toolbar tooltips and the dispatcher all ask
 * this module what the key is; nothing hardcodes a combination elsewhere.
 */

export * from "./shortcut-definitions";
export * from "./shortcut-keys";
export * from "./shortcut-store";
