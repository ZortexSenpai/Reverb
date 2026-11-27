// Device configs: named output setups the listener switches between in one go.
// These helpers return preference patches; the prefs store persists them, and
// the backend reopens the output when the setup they describe changes.

import type { DeviceConfig, HqPlayerEndpoint, OutputDevice, Preferences } from "./subsonic/types";

/** The port HQPlayer listens for control on unless told otherwise. */
export const HQPLAYER_DEFAULT_PORT = 4321;

type ConfigPrefs = Pick<Preferences, "deviceConfigs" | "activeDeviceConfig">;

/** The device config playing now; `null` means the system default. */
export function activeDeviceConfig(prefs: ConfigPrefs): DeviceConfig | null {
  return prefs.deviceConfigs.find((c) => c.id === prefs.activeDeviceConfig) ?? null;
}

/**
 * Save `config`: replace the one with its id, or add it. A new config becomes
 * the active one — making a config is almost always the step before playing
 * through it.
 */
export function saveDeviceConfig(prefs: ConfigPrefs, config: DeviceConfig): ConfigPrefs {
  if (prefs.deviceConfigs.some((c) => c.id === config.id)) {
    return {
      deviceConfigs: prefs.deviceConfigs.map((c) => (c.id === config.id ? config : c)),
      activeDeviceConfig: prefs.activeDeviceConfig,
    };
  }
  return { deviceConfigs: [...prefs.deviceConfigs, config], activeDeviceConfig: config.id };
}

/** Delete a config. Deleting the active one moves playback to the system default. */
export function deleteDeviceConfig(prefs: ConfigPrefs, id: string): ConfigPrefs {
  return {
    deviceConfigs: prefs.deviceConfigs.filter((c) => c.id !== id),
    activeDeviceConfig: prefs.activeDeviceConfig === id ? null : prefs.activeDeviceConfig,
  };
}

/**
 * Mirror the backend after `device` was unplugged: playback fell back to the
 * system default, so that is what is active now. A config switched to since
 * the device was asked for is left alone — it is what is playing. The config
 * keeps its device either way, so it works again once the device is back.
 */
export function afterDeviceLost(prefs: ConfigPrefs, device: string): ConfigPrefs {
  return activeDeviceConfig(prefs)?.device === device
    ? { ...prefs, activeDeviceConfig: null }
    : prefs;
}

/**
 * The device in a list of the ones plugged in. Settings older than device ids
 * held the display name, which the backend still resolves — so match on it too.
 */
function findDevice(devices: OutputDevice[], device: string): OutputDevice | undefined {
  return devices.find((d) => d.id === device) ?? devices.find((d) => d.label === device);
}

/**
 * How a config's device reads: its display name, or `null` for the system
 * default. `undefined` while it cannot be told — the list is still loading, or
 * the device is not plugged in.
 */
export function deviceLabel(
  devices: OutputDevice[] | undefined,
  device: string | null,
): string | null | undefined {
  if (device == null) return null;
  return devices && findDevice(devices, device)?.label;
}

/** How an HQPlayer address reads: the host, with the port only when it is not the usual one. */
export function hqplayerAddress({ host, port }: HqPlayerEndpoint): string {
  return port === HQPLAYER_DEFAULT_PORT ? host : `${host}:${port}`;
}

/**
 * Read a typed port. Blank is the default port; anything else must be a whole
 * number in range, or `null` to say it is not a port.
 */
export function parsePort(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === "") return HQPLAYER_DEFAULT_PORT;
  if (!/^\d+$/.test(trimmed)) return null;
  const port = Number(trimmed);
  return port >= 1 && port <= 65535 ? port : null;
}

/** Whether a config's device is plugged in. Unknown counts as yes. */
export function isConnected(devices: OutputDevice[] | undefined, device: string | null): boolean {
  return device == null || devices == null || findDevice(devices, device) != null;
}

/**
 * Whether exclusive access means anything on `device`. On Linux it does only
 * for a direct (hw:) device: a sound-server route has no card to take, so the
 * engine's exclusive path resolves to the shared default anyway. On macOS and
 * Windows it is a device-level mode that works on the default device too, and
 * no device is ever reported as direct — so only gate where the flag exists.
 * While the list is loading the answer is yes, rather than blinking the toggle
 * disabled.
 */
export function canGoExclusive(
  devices: OutputDevice[] | undefined,
  device: string | null,
): boolean {
  if (devices == null || !devices.some((d) => d.direct)) return true;
  return device != null && Boolean(findDevice(devices, device)?.direct);
}
