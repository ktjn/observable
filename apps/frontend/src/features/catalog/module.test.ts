import { validateCatalogSearch } from "./module";

describe("validateCatalogSearch", () => {
  it("keeps one explicit environment class and scope for the review view only", () => {
    expect(validateCatalogSearch({ view: "review" })).toMatchObject({ view: "review", env: undefined, scope: undefined });
    expect(validateCatalogSearch({ view: "review", env: "non_production", scope: "all" }))
      .toMatchObject({ env: "non_production", scope: "all" });
    expect(validateCatalogSearch({ view: "review", env: "staging", scope: "everything" }))
      .toMatchObject({ env: undefined, scope: undefined });
    expect(validateCatalogSearch({ view: "coverage", env: "non_production", scope: "all" }))
      .toMatchObject({ env: undefined, scope: undefined });
  });

  it("keeps overview sort, state filter and inspected service in the URL", () => {
    const selected = "00000000-0000-0000-0000-000000000011";
    expect(validateCatalogSearch({ view: "overview", env: "non_production", sort: "age", state: "degraded", selected }))
      .toMatchObject({ view: "overview", env: "non_production", sort: "age", state: "degraded", selected });
    expect(validateCatalogSearch({ view: "overview", sort: "state", state: "fine", selected: "x" }))
      .toMatchObject({ sort: undefined, state: undefined, selected: undefined });
    expect(validateCatalogSearch({ view: "review", sort: "age", selected }))
      .toMatchObject({ sort: undefined, selected: undefined });
  });
});
