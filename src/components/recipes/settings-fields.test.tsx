// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { NumberInput, SettingsField, TextInput, TimeInput } from "@/components/recipes/settings-fields";
import { Switch } from "@/components/ui/switch";

afterEach(cleanup);

describe("SettingsField", () => {
  it("shows the label and the hint", () => {
    render(
      <SettingsField label="Launch on login" hint="Starts RepoSync with the computer.">
        <Switch checked={false} onCheckedChange={() => {}} />
      </SettingsField>,
    );
    expect(screen.getByText("Launch on login")).toBeTruthy();
    expect(screen.getByText("Starts RepoSync with the computer.")).toBeTruthy();
  });

  it("names the control inside it from the label, and leaves the hint out of the name", () => {
    render(
      <SettingsField label="Launch on login" hint="Starts RepoSync with the computer.">
        <Switch checked={false} onCheckedChange={() => {}} />
      </SettingsField>,
    );
    expect(screen.getByRole("switch", { name: "Launch on login" })).toBeTruthy();
  });

  it("renders without a hint", () => {
    render(
      <SettingsField label="Version">
        <span>1.2.3</span>
      </SettingsField>,
    );
    expect(screen.getByText("Version")).toBeTruthy();
  });
});

describe("NumberInput", () => {
  it("reports a typed number, not a string", () => {
    const onChange = vi.fn();
    render(<NumberInput value={5} min={1} max={10} suffix="min" onChange={onChange} />);
    const input = screen.getByRole("spinbutton");
    expect((input as HTMLInputElement).value).toBe("5");
    fireEvent.change(input, { target: { value: "7" } });
    expect(onChange).toHaveBeenCalledWith(7);
    expect(screen.getByText("min")).toBeTruthy();
  });
});

describe("TextInput", () => {
  it("shows an empty box for null and reports null again when cleared", () => {
    const onChange = vi.fn();
    render(<TextInput value={null} placeholder="optional" onChange={onChange} />);
    const input = screen.getByPlaceholderText("optional") as HTMLInputElement;
    expect(input.value).toBe("");
    fireEvent.change(input, { target: { value: "abc" } });
    expect(onChange).toHaveBeenLastCalledWith("abc");
  });

  it("reports null when the text is cleared", () => {
    const onChange = vi.fn();
    render(<TextInput value="abc" onChange={onChange} />);
    fireEvent.change(screen.getByDisplayValue("abc"), { target: { value: "" } });
    expect(onChange).toHaveBeenCalledWith(null);
  });
});

describe("TimeInput", () => {
  it("shows a minute-of-day as HH:MM and reports minutes back", () => {
    const onChange = vi.fn();
    render(<TimeInput value={90} aria-label="Quiet from" onChange={onChange} />);
    const input = screen.getByLabelText("Quiet from") as HTMLInputElement;
    expect(input.value).toBe("01:30");
    fireEvent.change(input, { target: { value: "02:15" } });
    expect(onChange).toHaveBeenCalledWith(135);
  });
});
