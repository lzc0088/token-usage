import { test, expect } from "@playwright/test";

const POPOVER_WAIT = 5000; // wait for tokscale to load data

test.describe("Popover (main window)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Wait for the popover to be visible
    await page.waitForSelector(".popover", { timeout: POPOVER_WAIT });
  });

  test("shows hero with token stats", async ({ page }) => {
    // Hero section should be visible with the big token number
    const heroSection = page.locator(".hero-l");
    await expect(heroSection).toBeVisible();

    // Big token number should contain a number or dash (loading state)
    const bigNum = heroSection.locator(".big");
    await expect(bigNum).toBeVisible();
  });

  test("period switcher has three options", async ({ page }) => {
    const periodSwitcher = page.locator(".period");
    await expect(periodSwitcher).toBeVisible();

    // Check all three period buttons exist
    const dayBtn = page.locator("[data-testid='period-day']");
    const monthBtn = page.locator("[data-testid='period-month']");
    const totalBtn = page.locator("[data-testid='period-total']");

    await expect(dayBtn).toBeVisible();
    await expect(monthBtn).toBeVisible();
    await expect(totalBtn).toBeVisible();
  });

  test("switching period triggers data reload", async ({ page }) => {
    // Click month period
    await page.click("[data-testid='period-month']");
    // Brief wait for IPC round-trip
    await page.waitForTimeout(300);
    // The summary section should update (no error state)
    const loadError = page.locator("[data-testid='load-error']");
    await expect(loadError).not.toBeVisible();
  });

  test("segments render without error", async ({ page }) => {
    // Check that the segment bar is visible with the overview tab
    const segbar = page.locator("[data-testid='segbar']");
    await expect(segbar).toBeVisible();

    const overviewTab = page.locator("[data-testid='segment-ov']");
    await expect(overviewTab).toBeVisible();
  });
});
