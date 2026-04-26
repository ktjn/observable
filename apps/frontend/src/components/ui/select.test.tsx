import { fireEvent, render, screen } from "@testing-library/react";
import { Select, SelectOption } from "./select";

test("renders options and updates value", () => {
  render(
    <Select aria-label="Environment" defaultValue="dev">
      <SelectOption value="dev">dev</SelectOption>
      <SelectOption value="prod">prod</SelectOption>
    </Select>
  );

  const select = screen.getByRole("combobox", { name: "Environment" });
  expect(select).toHaveValue("dev");

  fireEvent.change(select, { target: { value: "prod" } });
  expect(select).toHaveValue("prod");
});
