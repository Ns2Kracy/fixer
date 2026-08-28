/// <reference types="node" />

import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

function requiredEnvironment(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} must be set by the E2E harness`);
  return value;
}

test("user reviews a local candidate and approves its bounded write", async ({
  page,
}) => {
  const password = requiredEnvironment("FIXER_E2E_PASSWORD");
  const mediaPath = requiredEnvironment("FIXER_E2E_MEDIA_PATH");
  const outputPath = requiredEnvironment("FIXER_E2E_OUTPUT_PATH");
  const sourceNames = ["In the Mood for Love (2000).mkv", "movie.nfo"].sort();
  const sourceContents = new Map(
    await Promise.all(
      sourceNames.map(
        async (name) => [name, await readFile(join(mediaPath, name))] as const,
      ),
    ),
  );
  expect((await readdir(mediaPath)).sort()).toEqual(sourceNames);

  const browserProblems: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") {
      browserProblems.push(`${message.type()}: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => {
    browserProblems.push(`pageerror: ${error.message}`);
  });

  await page.goto("/login");
  await page.getByLabel("Workspace password").fill(password);
  const loginResponse = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith("/api/v1/auth/login"),
  );
  await page.getByRole("button", { name: "Sign in" }).click();
  expect((await loginResponse).ok()).toBe(true);
  await expect(page).toHaveURL(/\/$/);

  await page.getByRole("link", { name: "Jobs" }).click();
  await expect(
    page.getByRole("heading", { name: "Scrape jobs" }),
  ).toBeVisible();
  await page.getByLabel("Media kind").selectOption("movie");
  await page.getByLabel("Media path").fill(mediaPath);
  await page.getByLabel("Allow approved writes").check();

  const createResponse = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith("/api/v1/jobs"),
  );
  await page.getByRole("button", { name: "Create job" }).click();
  const created = await createResponse;
  expect(created.ok()).toBe(true);
  const createdBody = (await created.json()) as { job: { id: number } };
  const jobId = createdBody.job.id;

  await page.goto(`/jobs/${jobId}`);
  const reviewLink = page.getByRole("link", { name: "Review metadata" });
  await expect(reviewLink).toBeVisible();
  await reviewLink.click();

  await expect(
    page.getByRole("heading", { name: "Review metadata" }),
  ).toBeVisible();
  await expect(
    page.getByRole("radio", { name: /^Select .+ from local$/ }),
  ).toBeChecked();
  await page
    .getByRole("button", { name: "Accept candidate and build plan" })
    .click();

  await expect(page).toHaveURL(new RegExp(`/jobs/${jobId}/plan$`));
  await expect(
    page.getByRole("heading", { name: "Output plan" }),
  ).toBeVisible();
  const outputDiff = page.locator(".output-diff");
  await expect(outputDiff.locator(".output-root code")).toHaveText(mediaPath);
  const operations = outputDiff.locator("ol > li");
  await expect(operations).toHaveCount(1);
  await expect(operations.locator(".operation-kind")).toHaveText(
    "Write metadata",
  );
  await expect(operations.locator("code")).toHaveText("movie.json");
  await expect(operations.locator("small")).toHaveText(/\d+ bytes prepared/);
  await page
    .getByRole("checkbox", {
      name: "I approve these filesystem operations",
    })
    .check();

  const executeResponse = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith(`/api/v1/jobs/${jobId}/execute`),
  );
  await page.getByRole("button", { name: "Execute approved plan" }).click();
  const executed = await executeResponse;
  expect(executed.ok()).toBe(true);
  const executedBody = (await executed.json()) as {
    job: { state: string };
  };
  expect(executedBody.job.state).toBe("completed");
  await expect(
    page.getByText(
      "Execution was accepted. Live progress is available on the job page.",
    ),
  ).toBeVisible();

  expect((await readdir(mediaPath)).sort()).toEqual(
    [...sourceNames, "movie.json"].sort(),
  );
  for (const [name, original] of sourceContents) {
    expect(await readFile(join(mediaPath, name))).toEqual(original);
  }
  const movie = JSON.parse(await readFile(outputPath, "utf8")) as {
    id: string;
    titles: Array<{ kind: string; language: string; value: string }>;
    releases: Array<{
      id: string;
      release_date: { year: number };
    }>;
  };
  expect(movie.id).toBe("nfo-花样年华");
  expect(movie.titles).toEqual([
    { kind: "tagged", language: "und", value: "花样年华" },
    {
      kind: "tagged",
      language: "en",
      value: "In the Mood for Love",
    },
  ]);
  expect(movie.releases).toHaveLength(1);
  expect(movie.releases[0]).toMatchObject({
    id: "nfo-花样年华-2000",
    release_date: { year: 2000 },
  });

  await page.goto(`/jobs/${jobId}`);
  await expect(page.getByText("Completed", { exact: true })).toBeVisible();
  expect(browserProblems).toEqual([]);
});
