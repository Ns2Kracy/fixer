/// <reference types="node" />

import { expect, test } from "@playwright/test";

function requiredEnvironment(name: string): string {
  const value = process.env[name];
  if (value === undefined || value === "")
    throw new Error(`${name} must be set by the E2E harness`);
  return value;
}

const viewportWidths = [320, 768, 1024, 1440] as const;

test.beforeEach(async ({ page }) => {
  await page.route("**/api/v1/health", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        schema_version: 1,
        status: "ok",
        version: "test",
      }),
    });
  });
});

test("theme follows the system, persists overrides, and stays responsive", async ({
  page,
}) => {
  const username = requiredEnvironment("FIXER_E2E_USERNAME");
  const password = requiredEnvironment("FIXER_E2E_PASSWORD");
  const statusResponse = await page.request.get("/api/v1/auth/status");
  expect(statusResponse.ok()).toBe(true);
  const status = (await statusResponse.json()) as {
    registration_required: boolean;
  };
  const authResponse = await page.request.post(
    status.registration_required
      ? "/api/v1/auth/register"
      : "/api/v1/auth/login",
    { data: { username, password } },
  );
  expect(authResponse.ok()).toBe(true);

  const browserProblems: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") {
      browserProblems.push(`${message.type()}: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => {
    browserProblems.push(`pageerror: ${error.message}`);
  });

  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.locator("body")).toHaveCSS(
    "background-color",
    "rgb(23, 26, 22)",
  );
  await expect(page.locator("body")).toHaveCSS("color", "rgb(241, 238, 230)");
  await expect(page.getByLabel("Theme")).toHaveValue("system");
  await expect(page.locator('meta[name="theme-color"]')).toHaveAttribute(
    "content",
    "#171a16",
  );

  await page.getByLabel("Theme").selectOption("light");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(page.locator("body")).toHaveCSS(
    "background-color",
    "rgb(243, 240, 232)",
  );
  await expect(page.locator("body")).toHaveCSS("color", "rgb(29, 33, 28)");
  const reviewWorkspace = page.getByRole("link", { name: "Review workspace" });
  await expect(reviewWorkspace).toHaveCSS(
    "background-color",
    "rgb(29, 33, 28)",
  );
  await expect(reviewWorkspace).toHaveCSS("color", "rgb(243, 240, 232)");
  await page.reload();
  await expect(page.getByLabel("Theme")).toHaveValue("light");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");

  await page.getByLabel("Theme").selectOption("system");
  await page.emulateMedia({ colorScheme: "light" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  for (const width of viewportWidths) {
    await page.setViewportSize({ width, height: 900 });
    await expect(
      page.getByRole("heading", {
        name: "Metadata work, without guesswork.",
      }),
    ).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
      `page should not overflow horizontally at ${width}px`,
    ).toBe(true);
  }

  expect(browserProblems).toEqual([]);
});
