#!/usr/bin/env tsx
import { createTestDriver } from "./test-driver-client";

const [action, selector, value] = process.argv.slice(2);

if (!action) {
  console.log(`Usage:
  test-driver snapshot
  test-driver getText "h1"
  test-driver count "button"
  test-driver click "[data-tid='e1']"
  test-driver fill "textarea" "hello"
  test-driver keypress "textarea" Enter
  test-driver waitForText "expected text"
  test-driver scroll down|up|top|bottom
  test-driver screenshot [output.png]`);
  process.exit(0);
}

try {
  const testDriver = await createTestDriver();
  let result: string | number;

  if (action === "screenshot") {
    result = await testDriver.screenshot(
      selector || `tests/app-e2e/screenshots/screenshot-${Date.now()}.png`,
    );
  } else if (action === "fill") {
    result = await testDriver.fill(selector, value);
  } else if (action === "keypress") {
    result = await testDriver.keypress(selector, value);
  } else if (action === "waitForText") {
    result = await testDriver.waitForText(selector);
  } else if (action === "scroll") {
    result = await testDriver.scroll(selector);
  } else if (action === "count") {
    result = await testDriver.count(selector);
  } else if (action === "click") {
    result = await testDriver.click(selector);
  } else if (action === "getText") {
    result = await testDriver.getText(selector);
  } else if (action === "snapshot") {
    result = await testDriver.snapshot();
  } else {
    console.error(`Unknown action: ${action}`);
    process.exit(1);
  }

  console.log(result);
  testDriver.close();
} catch (err) {
  console.error("Error:", (err as Error).message);
  process.exit(1);
}
