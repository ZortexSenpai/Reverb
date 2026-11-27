import { describe, expect, it } from "vitest";
import {
  activeDeviceConfig,
  afterDeviceLost,
  canGoExclusive,
  deleteDeviceConfig,
  deviceLabel,
  hqplayerAddress,
  isConnected,
  parsePort,
  saveDeviceConfig,
} from "./deviceConfigs";
import type { DeviceConfig, OutputDevice } from "./subsonic/types";

const desk: DeviceConfig = {
  id: "desk",
  name: "Desk DAC",
  device: "coreaudio:dac",
  matchSourceRate: true,
  exclusiveOutput: true,
  hqplayer: null,
};
const speakers: DeviceConfig = {
  id: "speakers",
  name: "Speakers",
  device: null,
  matchSourceRate: false,
  exclusiveOutput: false,
  hqplayer: null,
};
const prefs = { deviceConfigs: [desk, speakers], activeDeviceConfig: "desk" };

describe("activeDeviceConfig", () => {
  it("finds the config by id", () => {
    expect(activeDeviceConfig(prefs)).toBe(desk);
  });

  it("is the system default with no config active, or one that was deleted", () => {
    expect(activeDeviceConfig({ ...prefs, activeDeviceConfig: null })).toBeNull();
    expect(activeDeviceConfig({ ...prefs, activeDeviceConfig: "gone" })).toBeNull();
  });
});

describe("saveDeviceConfig", () => {
  it("adds a new config and makes it the active one", () => {
    const fresh = { ...speakers, id: "new", name: "Headphones" };
    expect(saveDeviceConfig(prefs, fresh)).toEqual({
      deviceConfigs: [desk, speakers, fresh],
      activeDeviceConfig: "new",
    });
  });

  it("edits a config in place without changing which one is active", () => {
    const renamed = { ...speakers, name: "Living room" };
    expect(saveDeviceConfig(prefs, renamed)).toEqual({
      deviceConfigs: [desk, renamed],
      activeDeviceConfig: "desk",
    });
  });
});

describe("deleteDeviceConfig", () => {
  it("keeps the active config when another one goes", () => {
    expect(deleteDeviceConfig(prefs, "speakers")).toEqual({
      deviceConfigs: [desk],
      activeDeviceConfig: "desk",
    });
  });

  it("falls back to the system default when the active one goes", () => {
    expect(deleteDeviceConfig(prefs, "desk")).toEqual({
      deviceConfigs: [speakers],
      activeDeviceConfig: null,
    });
  });
});

describe("afterDeviceLost", () => {
  it("moves to the system default and keeps the config's device", () => {
    const after = afterDeviceLost(prefs, "coreaudio:dac");
    expect(after.activeDeviceConfig).toBeNull();
    expect(after.deviceConfigs).toEqual([desk, speakers]);
  });

  it("leaves a config switched to since alone", () => {
    const switched = { ...prefs, activeDeviceConfig: "speakers" };
    expect(afterDeviceLost(switched, "coreaudio:dac")).toBe(switched);
  });
});

const fiio: OutputDevice = { id: "alsa:hw:CARD=1,DEV=0", label: "FiiO BTR17", direct: true };
const mac: OutputDevice = { id: "coreaudio:mac", label: "MacBook Pro Speakers", direct: false };

describe("devices", () => {
  it("names a device by id, or by the display name older settings held", () => {
    expect(deviceLabel([fiio], fiio.id)).toBe("FiiO BTR17");
    expect(deviceLabel([mac], "MacBook Pro Speakers")).toBe("MacBook Pro Speakers");
    expect(deviceLabel([mac], null)).toBeNull();
  });

  it("cannot name a device it cannot see", () => {
    expect(deviceLabel([mac], fiio.id)).toBeUndefined();
    expect(deviceLabel(undefined, fiio.id)).toBeUndefined();
  });

  it("reports an unplugged device, but not one it has not looked for yet", () => {
    expect(isConnected([mac], fiio.id)).toBe(false);
    expect(isConnected([mac, fiio], fiio.id)).toBe(true);
    expect(isConnected(undefined, fiio.id)).toBe(true);
    expect(isConnected([], null)).toBe(true);
  });
});

describe("canGoExclusive", () => {
  it("needs a direct device where devices are told apart that way (Linux)", () => {
    const linux: OutputDevice[] = [fiio];
    expect(canGoExclusive(linux, fiio.id)).toBe(true);
    expect(canGoExclusive(linux, null)).toBe(false);
    expect(canGoExclusive(linux, "alsa:pulse")).toBe(false);
  });

  it("works on any device where none is direct (macOS, Windows)", () => {
    expect(canGoExclusive([mac], null)).toBe(true);
    expect(canGoExclusive([mac], mac.id)).toBe(true);
  });

  it("does not disable the toggle while the list is loading", () => {
    expect(canGoExclusive(undefined, null)).toBe(true);
  });
});

describe("hqplayerAddress", () => {
  it("leaves out the usual port", () => {
    expect(hqplayerAddress({ host: "studio.local", port: 4321 })).toBe("studio.local");
  });

  it("shows any other port", () => {
    expect(hqplayerAddress({ host: "192.168.1.20", port: 4322 })).toBe("192.168.1.20:4322");
  });
});

describe("parsePort", () => {
  it("takes a blank field as the usual port", () => {
    expect(parsePort("")).toBe(4321);
    expect(parsePort("  ")).toBe(4321);
  });

  it("reads a port in range", () => {
    expect(parsePort(" 4322 ")).toBe(4322);
    expect(parsePort("65535")).toBe(65535);
  });

  it("refuses anything that is not a port", () => {
    expect(parsePort("0")).toBeNull();
    expect(parsePort("65536")).toBeNull();
    expect(parsePort("43.21")).toBeNull();
    expect(parsePort("-1")).toBeNull();
    expect(parsePort("http")).toBeNull();
  });
});
