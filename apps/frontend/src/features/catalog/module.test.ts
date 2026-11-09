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
});
