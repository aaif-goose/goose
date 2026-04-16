import { describe, it, expect } from "vitest";
import { useTestDriver } from "./lib/setup";

describe("Chat", () => {
  const testDriver = useTestDriver();

  it("returns formatted date when asked", async () => {
    await testDriver.fill(
      'textarea[placeholder*="Message Goose"]',
      'Show me the date of Jan 26 2025 in format of "dd-mm-yyyy"',
    );
    await testDriver.keypress(
      'textarea[placeholder*="Message Goose"]',
      "Enter",
    );

    const bodyText = await testDriver.waitForText("26-01-2025", {
      timeout: 30000,
    });
    expect(bodyText).toContain("26-01-2025");
  });
});
