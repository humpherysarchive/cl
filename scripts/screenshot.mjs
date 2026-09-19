/**
 * Render the UI to PNGs so interface changes can be reviewed without a build.
 *
 *   node scripts/screenshot.mjs [outDir]
 *
 * Uses whatever Chromium Playwright can find. Set CHROMIUM_PATH to override,
 * which is needed in containers that ship a browser Playwright didn't install.
 */
import { chromium } from "playwright";
import { createServer } from "vite";
import { mkdir } from "node:fs/promises";

const outDir = process.argv[2] ?? "screenshots";
await mkdir(outDir, { recursive: true });

const server = await createServer({ server: { port: 5199, strictPort: true } });
await server.listen();

const browser = await chromium.launch(
  process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {},
);
const page = await browser.newPage({
  viewport: { width: 1180, height: 760 },
  deviceScaleFactor: 2,
});

/** Pin settings before load so "Auto" can't pick the theme for us. */
async function withSettings(settings) {
  await page.goto("http://localhost:5199", { waitUntil: "domcontentloaded" });
  await page.evaluate(
    (s) => localStorage.setItem("cl.settings.v1", JSON.stringify(s)),
    settings,
  );
  await page.reload({ waitUntil: "networkidle" });
  await page.evaluate(() => document.fonts.ready);
}

async function shot(name) {
  await page.waitForTimeout(700); // let the enter animations settle
  await page.screenshot({ path: `${outDir}/${name}.png` });
  console.log(`  ${name}.png`);
}

await withSettings({ theme: "dark" });
await shot("dark-photos");
await page.getByRole("button", { name: "Messages" }).click();
await shot("dark-messages");

await withSettings({ theme: "light" });
await page.getByRole("button", { name: "Notes" }).click();
await shot("light-notes");
await page.getByRole("button", { name: "Settings" }).click();
await shot("light-settings");

await page.getByRole("switch", { name: "Developer mode" }).click();
await page.waitForTimeout(500);
await page.evaluate(() =>
  document.querySelector(".sheet__body").scrollTo({ top: 99999 }),
);
await shot("light-developer");

// Import sheet, first step.
await withSettings({ theme: "dark" });
await page.getByRole("button", { name: "Import", exact: true }).click();
await shot("dark-import");

await browser.close();
await server.close();
