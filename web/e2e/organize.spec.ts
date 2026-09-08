import { expect, test } from "@playwright/test";

const activeJob = {
  id: 7,
  state: "awaiting_confirmation",
  input: {
    schema_version: 1,
    input_path: "Videos/赛博朋克 (2022)",
    media_kind: "television",
    apply: true,
    organization: {
      destination_path: "Fixer/赛博朋克 (2022)",
      placement: "hardlink",
      auto_execute: false,
    },
  },
  created_at_ms: 1,
  updated_at_ms: 2,
};

const completedJob = {
  ...activeJob,
  id: 8,
  state: "completed",
  input: { ...activeJob.input, input_path: "Videos/Arrival (2016)" },
  execution: { completed_operations: 4, failed_operations: 0 },
};

test.beforeEach(async ({ page }) => {
  await page.route("**/api/v1/auth/status", (route) =>
    route.fulfill({
      json: {
        schema_version: 1,
        registration_required: false,
        authenticated: true,
        username: "admin",
      },
    }),
  );
  await page.route("**/api/v1/library/roots", (route) =>
    route.fulfill({ json: { schema_version: 1, roots: [] } }),
  );
  await page.route("**/api/v1/ingestion-rules", (route) =>
    route.fulfill({ json: { schema_version: 1, rules: [] } }),
  );
  await page.route("**/api/v1/jobs/7/review*", (route) =>
    route.fulfill({
      json: {
        schema_version: 1,
        job_id: 7,
        selected_candidate_index: 0,
        candidates: [
          {
            index: 0,
            media_kind: "television",
            provider: "tmdb",
            external_id: { namespace: "tmdb", value: "112836" },
            title: "赛博朋克：边缘行者",
            year: 2022,
            score: 98,
            evidence: [{ kind: "title", points: 80, detail: "title matched" }],
            evidence_truncated: false,
          },
        ],
        candidates_truncated: false,
        warnings: [],
        warnings_truncated: false,
        conflicts: [],
        conflicts_truncated: false,
      },
    }),
  );
  await page.route("**/api/v1/jobs/7", (route) =>
    route.fulfill({ json: { schema_version: 1, job: activeJob } }),
  );
  await page.route("**/api/v1/jobs?*", (route) =>
    route.fulfill({
      json: {
        schema_version: 1,
        jobs: [activeJob, completedJob],
        has_more: false,
      },
    }),
  );
});

test("organize workspace separates active work, records, and settings", async ({
  page,
}) => {
  await page.goto("/");

  const primary = page.getByRole("navigation", { name: "主导航" });
  await expect(primary.getByRole("link")).toHaveText([
    "整理",
    "整理记录",
    "设置",
  ]);
  await expect(page.getByText("赛博朋克 (2022)")).toBeVisible();
  await expect(page.getByText("Arrival (2016)")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "选择目录并整理" }),
  ).toBeVisible();
  await expect(page.getByLabel("Media kind")).toHaveCount(0);

  await page.getByRole("link", { name: "整理记录", exact: true }).click();
  await expect(page.getByRole("heading", { name: "整理记录" })).toBeVisible();
  await expect(page.getByText("Arrival (2016)")).toBeVisible();
  await expect(page.getByRole("button", { name: "开始识别" })).toHaveCount(0);

  await page.getByRole("link", { name: "设置", exact: true }).click();
  const settings = page.getByRole("navigation", { name: "设置分类" });
  await expect(settings.getByRole("link")).toHaveText([
    "通用与刮削设置",
    "自动整理目录",
    "数据源状态",
    "命名规则",
    "浏览文件",
  ]);
});

test("work detail foregrounds identity and keeps pipeline secondary on mobile", async ({
  page,
}) => {
  await page.setViewportSize({ width: 320, height: 900 });
  await page.goto("/jobs/7");

  await expect(
    page.getByRole("heading", { name: "赛博朋克 (2022)" }),
  ).toBeVisible();
  await expect(page.getByText("赛博朋克：边缘行者")).toBeVisible();
  await expect(page.getByText("Fixer/赛博朋克 (2022)")).toBeVisible();
  const pipeline = page.getByText("技术详情 · 处理流水线").locator("..");
  await expect(pipeline).not.toHaveAttribute("open");
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
});
