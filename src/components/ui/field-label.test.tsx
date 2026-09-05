// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { FieldLabelContext } from "@/components/ui/field-label";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";

/**
 * BL-NI-90 (Settings switches have no accessible name). Six switches shipped
 * announcing as "switch, on" with nothing to say which setting, and one of
 * them went two months without appearing in the backlog row tracking the
 * others. A count is exactly the kind of fact that rots, so these assert the
 * MECHANISM instead: a control inside a labelled region takes that region's
 * name, whatever the controls happen to be on any given day.
 *
 * Assertions are on the accessible name, never on attributes, so the fix is
 * free to change how it spells the association.
 */

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  // A miniature of the Settings `Field`: label text carrying an id, control
  // rendered inside the provider. Kept local so this file tests the contract
  // rather than one screen's markup.
  return (
    <div>
      <div id="field-label">{label}</div>
      <FieldLabelContext.Provider value="field-label">{children}</FieldLabelContext.Provider>
    </div>
  );
}

afterEach(cleanup);

describe("field label association", () => {
  it("names a switch from the enclosing field's visible label", () => {
    render(
      <Field label="Launch on login">
        <Switch checked={false} onCheckedChange={() => {}} />
      </Field>,
    );
    expect(screen.getByRole("switch", { name: "Launch on login" })).toBeTruthy();
  });

  it("names an input the same way, so the fix is not switch-only", () => {
    render(
      <Field label="Activity retention">
        <Input type="number" value={30} onChange={() => {}} />
      </Field>,
    );
    expect(screen.getByRole("spinbutton", { name: "Activity retention" })).toBeTruthy();
  });

  it("lets an explicit aria-label win over the field label", () => {
    render(
      <Field label="Field label">
        <Switch checked={false} onCheckedChange={() => {}} aria-label="Explicit name" />
      </Field>,
    );
    expect(screen.getByRole("switch", { name: "Explicit name" })).toBeTruthy();
  });

  it("leaves a switch outside any field unnamed rather than inventing a name", () => {
    // The escape-hatch case. Nothing should fabricate a name here; the point
    // is that the mechanism does not reach where there is no label to reach
    // for.
    render(<Switch checked={false} onCheckedChange={() => {}} />);
    expect(screen.getByRole("switch").getAttribute("aria-labelledby")).toBeNull();
  });
});
