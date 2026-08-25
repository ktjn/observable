import { describe, expect, it } from "vitest";
import { SqliteAuthRepository } from "./sqliteAuthRepository";

describe("SqliteAuthRepository", () => {
  it("reads the seeded browser identity and tenant membership", async () => {
    const repository = await SqliteAuthRepository.open();

    expect(repository.me()).toEqual({
      user_id: "playground-user",
      email: "playground@local",
      tenants: [{ tenant_id: "00000000-0000-0000-0000-000000000001", role: "admin" }],
    });
  });
});
