import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { IconArrowNarrowLeft } from "@tabler/icons-react";
import { Button } from "./button";

describe("Button", () => {
  it("applies the button size to unsized icons", () => {
    render(
      <Button size="sm" leftIcon={<IconArrowNarrowLeft data-testid="icon" />}>
        Back
      </Button>,
    );

    expect(screen.getByTestId("icon")).toHaveClass("size-3");
  });

  it("preserves an explicit icon class size", () => {
    render(
      <Button
        size="sm"
        leftIcon={<IconArrowNarrowLeft data-testid="icon" className="size-4" />}
      >
        Back
      </Button>,
    );

    expect(screen.getByTestId("icon")).toHaveClass("size-4");
    expect(screen.getByTestId("icon")).not.toHaveClass("size-3");
  });

  it("preserves an explicit icon size prop", () => {
    render(
      <Button
        size="sm"
        leftIcon={<IconArrowNarrowLeft data-testid="icon" size={18} />}
      >
        Back
      </Button>,
    );

    expect(screen.getByTestId("icon")).toHaveAttribute("width", "18");
    expect(screen.getByTestId("icon")).toHaveAttribute("height", "18");
    expect(screen.getByTestId("icon")).not.toHaveClass("size-3");
  });

  it("renders the back variant with its default chevron icon", () => {
    render(
      <Button variant="back" size="sm">
        Back
      </Button>,
    );

    const button = screen.getByRole("button", { name: "Back" });
    const icon = button.querySelector("svg");

    expect(button).toHaveClass(
      "h-8",
      "px-3",
      "text-xs",
      "text-muted-foreground",
    );
    expect(icon).not.toBeNull();
    expect(icon).toHaveClass("size-3");
  });
});
