import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.join(scriptDir, '..');
const require = createRequire(path.join(repoRoot, 'ui', 'package.json'));
const puppeteer = require('puppeteer-core');

const BASE_URL = process.env.GALE_UI_SMOKE_BASE_URL || 'http://127.0.0.1:5250';
const CHROME_PATH = process.env.GALE_UI_SMOKE_CHROME;
const SENSOR_LABEL = 'CPUTIN';
const SENSOR_VALUE_TEXT = '45.0';
const CONTROL_LABEL = 'nct6798 pwm1';

if (!CHROME_PATH) {
  console.error('GALE_UI_SMOKE_CHROME is not set');
  process.exit(1);
}

const results = [];

function record(name, fn) {
  return fn()
    .then(() => {
      results.push({ name, ok: true });
      console.log(`PASS ${name}`);
    })
    .catch((error) => {
      results.push({ name, ok: false, error });
      console.log(`FAIL ${name}: ${error.message}`);
    });
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function waitForSensorRow(page, label, expected, timeout) {
  await page.waitForFunction(
    (label, expected) => {
      const rows = Array.from(document.querySelectorAll('table tbody tr'));
      return rows.some((row) => {
        const l = row.querySelector('td.label');
        const v = row.querySelector('td.value');
        return l && v && l.textContent.trim() === label && v.textContent.includes(expected);
      });
    },
    { timeout },
    label,
    expected
  );
}

async function findControlCard(page, label) {
  const handle = await page.evaluateHandle((label) => {
    const cards = Array.from(document.querySelectorAll('.card.control'));
    return cards.find((card) => {
      const el = card.querySelector('.control-head .label');
      return el && el.textContent.trim() === label;
    }) || null;
  }, label);
  const element = handle.asElement();
  if (!element) {
    throw new Error(`control card for "${label}" not found`);
  }
  return element;
}

async function countCurvePoints(page) {
  return page.$$eval('svg.graph circle.curve-point', (circles) => circles.length);
}

async function main() {
  const browser = await puppeteer.launch({
    executablePath: CHROME_PATH,
    headless: true,
    args: ['--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage'],
  });

  let dialogMessage = null;
  try {
    const page = await browser.newPage();
    page.on('dialog', async (dialog) => {
      dialogMessage = dialog.message();
      await dialog.dismiss();
    });

    await record('dashboard loads', async () => {
      await page.goto(BASE_URL, { waitUntil: 'networkidle0' });
      await page.waitForSelector('h2', { timeout: 5000 });
    });

    await record('websocket connects (live indicator)', async () => {
      await page.waitForSelector('.dot.live', { timeout: 5000 });
    });

    await record('dashboard renders sensor value 45.0', async () => {
      await waitForSensorRow(page, SENSOR_LABEL, SENSOR_VALUE_TEXT, 5000);
    });

    await record('manual override badge appears within 3s of Apply', async () => {
      const card = await findControlCard(page, CONTROL_LABEL);
      const numberInput = await card.$('input[type="number"]');
      await numberInput.evaluate((el) => {
        el.value = '77';
        el.dispatchEvent(new Event('input', { bubbles: true }));
      });
      const applyButton = await card.$('button');
      await applyButton.click();
      await page.waitForFunction(
        (label) => {
          const cards = Array.from(document.querySelectorAll('.card.control'));
          const card = cards.find((c) => {
            const el = c.querySelector('.control-head .label');
            return el && el.textContent.trim() === label;
          });
          return !!card && !!card.querySelector('.badge');
        },
        { timeout: 3000 },
        CONTROL_LABEL
      );
    });

    let pointsBeforeDrag = 0;
    await record('navigate to Curves page', async () => {
      const clicked = await page.evaluate(() => {
        const button = Array.from(document.querySelectorAll('nav button')).find(
          (b) => b.textContent.trim() === 'Curves'
        );
        if (!button) return false;
        button.click();
        return true;
      });
      assert(clicked, 'Curves nav button not found');
      await page.waitForSelector('svg.graph', { timeout: 5000 });
      pointsBeforeDrag = await countCurvePoints(page);
      assert(pointsBeforeDrag >= 2, 'expected at least 2 curve points to start');
    });

    await record('point drag via mouse events adds no stray point', async () => {
      const circle = await page.$('svg.graph circle.curve-point');
      assert(circle, 'no curve point circle found');
      const box = await circle.boundingBox();
      assert(box, 'curve point has no bounding box');
      const startX = box.x + box.width / 2;
      const startY = box.y + box.height / 2;
      await page.mouse.move(startX, startY);
      await page.mouse.down();
      await page.mouse.move(startX + 30, startY - 15, { steps: 10 });
      await page.mouse.up();
      const pointsAfterDrag = await countCurvePoints(page);
      assert(
        pointsAfterDrag === pointsBeforeDrag,
        `point count changed from ${pointsBeforeDrag} to ${pointsAfterDrag}`
      );
    });

    await record('unsaved-edits guard: cancel keeps edits and stays on Curves', async () => {
      dialogMessage = null;
      const pointsBeforeNav = await countCurvePoints(page);
      const clicked = await page.evaluate(() => {
        const button = Array.from(document.querySelectorAll('nav button')).find(
          (b) => b.textContent.trim() === 'Dashboard'
        );
        if (!button) return false;
        button.click();
        return true;
      });
      assert(clicked, 'Dashboard nav button not found');
      assert(dialogMessage !== null, 'expected a confirm() dialog to fire');
      assert(dialogMessage.toLowerCase().includes('unsaved'), `unexpected dialog text: ${dialogMessage}`);
      const stillOnCurves = await page.$('svg.graph');
      assert(stillOnCurves, 'expected to remain on Curves after cancelling navigation');
      const pointsAfterNav = await countCurvePoints(page);
      assert(
        pointsAfterNav === pointsBeforeNav,
        `edits were lost across the cancelled navigation: ${pointsBeforeNav} -> ${pointsAfterNav}`
      );
    });

    await record('Save round-trips 204', async () => {
      const [response] = await Promise.all([
        page.waitForResponse(
          (res) => res.url().endsWith('/api/config') && res.request().method() === 'PUT',
          { timeout: 5000 }
        ),
        page.evaluate(() => {
          const button = Array.from(document.querySelectorAll('.actions button')).find(
            (b) => b.textContent.trim() === 'Save'
          );
          button.click();
        }),
      ]);
      assert(response.status() === 204, `expected 204, got ${response.status()}`);
    });
  } finally {
    await browser.close();
  }
}

main()
  .catch((error) => {
    results.push({ name: 'fatal', ok: false, error });
    console.error(`FATAL ${error.stack || error.message}`);
  })
  .finally(() => {
    const failures = results.filter((r) => !r.ok);
    console.log(`${results.length - failures.length}/${results.length} checks passed`);
    process.exit(failures.length > 0 ? 1 : 0);
  });
