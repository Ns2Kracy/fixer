/// <reference types="node" />

import { readdir, readFile } from "node:fs/promises";
import { basename, join, relative } from "node:path";

import { expect, test, type Page } from "@playwright/test";

function requiredEnvironment(name: string): string {
  const value = process.env[name];
  if (value === undefined || value === "")
    throw new Error(`${name} must be set by the E2E harness`);
  return value;
}

async function selectDirectory(
  page: Page,
  triggerName: string,
  segments: string[],
): Promise<void> {
  const trigger = page.getByRole("button", { name: triggerName });
  await trigger.press("Enter");
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();

  const root = dialog.getByRole("button", { name: "library", exact: true });
  await root.press("Enter");
  for (const segment of segments) {
    const directory = dialog.getByRole("button", {
      name: segment,
      exact: true,
    });
    await directory.press("Enter");
  }

  const select = dialog.getByRole("button", { name: "Select this folder" });
  await expect(select).toBeEnabled();
  await select.press("Enter");
  await expect(dialog).toHaveCount(0);
}

async function filesBelow(root: string): Promise<string[]> {
  const files: string[] = [];
  const pending = [root];
  while (pending.length > 0) {
    const directory = pending.pop();
    if (directory === undefined) break;
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) pending.push(path);
      else if (entry.isFile()) files.push(path);
    }
  }
  return files.sort();
}

test("user creates an automatic folder rule with directory pickers", async ({
  page,
}) => {
  const username = requiredEnvironment("FIXER_E2E_USERNAME");
  const password = requiredEnvironment("FIXER_E2E_PASSWORD");
  const mediaPath = requiredEnvironment("FIXER_E2E_MEDIA_PATH");
  const destinationPath = requiredEnvironment("FIXER_E2E_DESTINATION_PATH");
  const sourceNames = ["In the Mood for Love (2000).mkv", "movie.nfo"].sort();
  const sourceContents = new Map(
    await Promise.all(
      sourceNames.map(
        async (name) => [name, await readFile(join(mediaPath, name))] as const,
      ),
    ),
  );
  expect((await readdir(mediaPath)).sort()).toEqual(sourceNames);
  expect(await readdir(destinationPath)).toEqual([]);

  const browserProblems: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") {
      browserProblems.push(`${message.type()}: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => {
    browserProblems.push(`pageerror: ${error.message}`);
  });

  const statusResponse = await page.request.get("/api/v1/auth/status");
  expect(statusResponse.ok()).toBe(true);
  const status = (await statusResponse.json()) as {
    registration_required: boolean;
  };

  await page.goto("/login");
  await expect(
    page.getByRole("complementary", { name: "Primary navigation" }),
  ).toHaveCount(0);
  await page.getByLabel("Username").fill(username);
  await page.getByLabel("Password", { exact: true }).fill(password);

  const endpoint = status.registration_required
    ? "/api/v1/auth/register"
    : "/api/v1/auth/login";
  const authenticationResponse = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith(endpoint),
  );
  if (status.registration_required) {
    await page.getByLabel("Confirm password").fill(password);
    await page.getByRole("button", { name: "Create administrator" }).click();
  } else {
    await page.getByRole("tab", { name: "Sign in" }).click();
    await page.getByRole("button", { name: "Sign in" }).click();
  }
  expect((await authenticationResponse).ok()).toBe(true);
  await expect(page).toHaveURL(/\/$/u);

  await page.getByRole("link", { name: "Folders", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Folders" })).toBeVisible();
  expect(await page.locator("body").innerText()).not.toMatch(/workspace/iu);
  await page.getByRole("button", { name: "Add folder rule" }).click();
  await page.getByLabel("Rule name").fill("Incoming movies");
  await selectDirectory(page, "Choose source", ["movie"]);
  await selectDirectory(page, "Choose destination", ["organized"]);

  const save = page.getByRole("button", { name: "Save rule" });
  await expect(save).toBeDisabled();
  await page.getByLabel("Organization method").selectOption("copy");
  await expect(save).toBeEnabled();

  const createResponse = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith("/api/v1/ingestion-rules"),
  );
  await save.click();
  const created = await createResponse;
  expect(created.ok()).toBe(true);
  const createdBody = (await created.json()) as {
    rule: {
      source: { path: string };
      destination: { path: string };
      placement: string;
      status: string;
    };
  };
  expect(createdBody.rule).toMatchObject({
    source: { path: "movie" },
    destination: { path: "organized" },
    placement: "copy",
    status: "watching",
  });

  const rule = page
    .getByRole("heading", { name: "Incoming movies" })
    .locator("xpath=ancestor::article");
  await expect(rule).toContainText("movie → organized");
  await expect(rule.getByText("Watching", { exact: true })).toBeVisible();
  await expect(rule).toContainText("Automatic media · copy");

  for (const width of [320, 768, 1024, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    await expect(page.getByRole("heading", { name: "Folders" })).toBeVisible();
    await expect(rule).toBeVisible();
  }

  let completedJob:
    | {
        id: number;
        state: string;
        input: {
          organization?: {
            placement: string;
            origin_rule_id: number | null;
            auto_execute: boolean;
          };
        };
      }
    | undefined;
  await expect
    .poll(
      async () => {
        const response = await page.request.get("/api/v1/jobs");
        if (!response.ok()) return `http-${response.status()}`;
        const body = (await response.json()) as {
          jobs: (typeof completedJob)[];
        };
        completedJob = body.jobs[0];
        return completedJob?.state ?? "missing";
      },
      { timeout: 20_000 },
    )
    .toBe("completed");
  expect(completedJob?.input.organization).toMatchObject({
    placement: "copy",
    auto_execute: true,
  });
  expect(completedJob?.input.organization?.origin_rule_id).not.toBeNull();

  const destinationFiles = await filesBelow(destinationPath);
  const mediaFiles = destinationFiles.filter((path) => path.endsWith(".mkv"));
  expect(mediaFiles).toHaveLength(1);
  expect(await readFile(mediaFiles[0])).toEqual(
    sourceContents.get("In the Mood for Love (2000).mkv"),
  );
  const outputPath = destinationFiles.find(
    (path) => basename(path) === "movie.json",
  );
  expect(outputPath).toBeDefined();
  const movie = JSON.parse(await readFile(outputPath!, "utf8")) as {
    id: string;
    titles: Array<{ kind: string; language: string; value: string }>;
    releases: Array<{ id: string; release_date: { year: number } }>;
  };
  expect(movie.id).toBe("nfo-花样年华");
  expect(movie.titles).toContainEqual({
    kind: "tagged",
    language: "en",
    value: "In the Mood for Love",
  });
  expect(movie.releases[0]).toMatchObject({
    id: "nfo-花样年华-2000",
    release_date: { year: 2000 },
  });
  expect(relative(destinationPath, mediaFiles[0])).not.toMatch(/^\.\./u);

  expect((await readdir(mediaPath)).sort()).toEqual(sourceNames);
  for (const [name, original] of sourceContents) {
    expect(await readFile(join(mediaPath, name))).toEqual(original);
  }

  await page.goto(`/jobs/${completedJob!.id}`);
  await expect(page.getByText("Completed", { exact: true })).toBeVisible();
  expect(browserProblems).toEqual([]);
});
