import { beforeEach, describe, expect, it } from "vitest";
import { useUiStore } from "./uiStore";

beforeEach(() => {
  useUiStore.setState({ queuePanelOpen: false, queuePanelTab: "queue" });
});

describe("queue panel tab", () => {
  it("reopens the panel on the tab used last", () => {
    const ui = useUiStore.getState();
    ui.setQueuePanelOpen(true);
    ui.setQueuePanelTab("playHistory");
    ui.setQueuePanelOpen(false);
    ui.setQueuePanelOpen(true);

    expect(useUiStore.getState().queuePanelTab).toBe("playHistory");
  });
});
