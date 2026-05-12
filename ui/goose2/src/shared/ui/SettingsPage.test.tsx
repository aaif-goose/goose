import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SettingsPage } from "./SettingsPage";

describe("SettingsPage", () => {
  it("renders title-only headers", () => {
    render(<SettingsPage title="General" />);

    expect(
      screen.getByRole("heading", { name: "General" }),
    ).toBeInTheDocument();
  });

  it("renders description, actions, controls, and children", () => {
    render(
      <SettingsPage
        title="Extensions"
        description="Manage extensions"
        actions={<button type="button">Add</button>}
        controls={<input aria-label="Search extensions" />}
        contentClassName="custom-content"
      >
        <div>Extension list</div>
      </SettingsPage>,
    );

    expect(screen.getByText("Manage extensions")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add" })).toBeInTheDocument();
    expect(screen.getByLabelText("Search extensions")).toBeInTheDocument();
    expect(screen.getByText("Extension list").parentElement).toHaveClass(
      "custom-content",
    );
  });
});
