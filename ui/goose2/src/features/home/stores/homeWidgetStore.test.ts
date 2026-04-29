import { beforeEach, describe, expect, it } from "vitest";
import {
  createDefaultHomeWidgets,
  useHomeWidgetStore,
} from "./homeWidgetStore";

function resetStore() {
  useHomeWidgetStore.setState({ instances: [] });
}

describe("homeWidgetStore", () => {
  beforeEach(() => {
    localStorage.clear();
    resetStore();
  });

  it("creates the expected first-load widgets", () => {
    expect(createDefaultHomeWidgets().map((widget) => widget.type)).toEqual([
      "cube",
      "clock",
      "agentPin",
    ]);
  });

  it("adds widgets centered on the click point", () => {
    useHomeWidgetStore
      .getState()
      .addWidget("clock", 130, 66, undefined, { width: 400, height: 300 });

    expect(useHomeWidgetStore.getState().instances).toMatchObject([
      { type: "clock", x: 10, y: 0, z: 1 },
    ]);
  });

  it("moves widgets within canvas bounds", () => {
    useHomeWidgetStore
      .getState()
      .addWidget("clock", 130, 66, undefined, { width: 400, height: 300 });

    const id = useHomeWidgetStore.getState().instances[0].id;
    useHomeWidgetStore
      .getState()
      .moveWidget(id, 500, 500, { width: 400, height: 300 });

    expect(useHomeWidgetStore.getState().instances[0]).toMatchObject({
      x: 160,
      y: 60,
    });
  });

  it("bumps stacking order, updates state, and removes widgets", () => {
    useHomeWidgetStore.getState().addWidget("stickyNote", 200, 200);
    useHomeWidgetStore.getState().addWidget("weather", 300, 300);
    const [note, weather] = useHomeWidgetStore.getState().instances;

    useHomeWidgetStore.getState().bumpZ(note.id);
    useHomeWidgetStore.getState().updateWidgetState(note.id, { text: "hello" });
    useHomeWidgetStore.getState().removeWidget(weather.id);

    expect(useHomeWidgetStore.getState().instances).toMatchObject([
      { id: note.id, z: 3, state: { text: "hello" } },
    ]);
  });

  it("ignores unknown catalog types", () => {
    useHomeWidgetStore.getState().addWidget("missing", 10, 10);

    expect(useHomeWidgetStore.getState().instances).toEqual([]);
  });
});
