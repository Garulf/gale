import { deepStrictEqual } from 'node:assert/strict';
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
const SENSOR_NODE = 'sensor:hwmon/nct6798';
const TEMP1 = 'hwmon/nct6798/temp1';
const TEMP2 = 'hwmon/nct6798/temp2';
const CONTROL_NODE = 'control:hwmon/nct6798';
const TEMP3 = 'hwmon/nct6798/temp3';
const TEMP5 = 'hwmon/nct6798/temp5';
const PWM3 = 'hwmon/nct6798/pwm3';
const PWM5 = 'hwmon/nct6798/pwm5';
const CONTROL_EDGE_ID = 'curve:cpu:out->control:hwmon/nct6798:hwmon/nct6798/pwm1';
const LATE_SENSOR_EDGE_ID = `${SENSOR_NODE}:${TEMP5}->curve:late:sensor`;
const LATE_CONTROL_EDGE_ID = `curve:late:out->${CONTROL_NODE}:${PWM5}`;
const NEW_SENSOR_NAME = 'gpu_hot';

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

function handleSelector(nodeId, handleId) {
  return `[data-node-id="${nodeId}"] [data-handleid="${handleId}"]`;
}

async function centerOf(page, selector) {
  const element = await page.$(selector);
  assert(element, `${selector} not found`);
  const box = await element.boundingBox();
  assert(box, `${selector} has no bounding box`);
  const center = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const exposed = await element.evaluate(
    (el, point) => el.contains(document.elementFromPoint(point.x, point.y)),
    center
  );
  assert(exposed, `${selector} is covered by another element at its center`);
  return center;
}

async function fitView(page) {
  await page.evaluate(() => {
    Array.from(document.querySelectorAll('.gale-toolbar button'))
      .find((b) => b.textContent.trim() === 'Fit view')
      .click();
  });
}

async function dragConnection(page, sourceSelector, targetSelector) {
  const from = await centerOf(page, sourceSelector);
  const to = await centerOf(page, targetSelector);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 10 });
  await page.mouse.up();
}

async function saveGraph(page) {
  const [response] = await Promise.all([
    page.waitForResponse(
      (res) => res.url().endsWith('/api/config') && res.request().method() === 'PUT',
      { timeout: 5000 }
    ),
    page.evaluate(() => {
      document.querySelector('[data-testid="graph-save"]').click();
    }),
  ]);
  assert(response.status() === 204, `expected 204, got ${response.status()}`);
  await page.waitForFunction(
    () => !document.querySelector('[data-testid="graph-save"]').disabled,
    { timeout: 5000 }
  );
}

async function fetchConfig() {
  const response = await fetch(`${BASE_URL}/api/config`);
  assert(response.ok, `GET /api/config returned ${response.status}`);
  return response.json();
}

async function virtualNodeIds(page) {
  return page.$$eval('[data-node-id^="virtual:"]', (nodes) =>
    nodes.map((node) => node.getAttribute('data-node-id'))
  );
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
    await page.setViewport({ width: 1600, height: 1000 });
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
    await record('navigate to Graph page', async () => {
      const clicked = await page.evaluate(() => {
        const button = Array.from(document.querySelectorAll('nav button')).find(
          (b) => b.textContent.trim() === 'Graph'
        );
        if (!button) return false;
        button.click();
        return true;
      });
      assert(clicked, 'Graph nav button not found');
      await page.waitForSelector('.svelte-flow', { timeout: 5000 });
    });

    await record('canvas renders the example device, virtual and curve nodes', async () => {
      await page.waitForFunction(
        () =>
          document.querySelector('[data-node-id="sensor:hwmon/nct6798"]') &&
          document.querySelector('[data-node-id="control:hwmon/nct6798"]') &&
          document.querySelector('[data-node-id="virtual:combined"]') &&
          document.querySelector('[data-node-id="curve:cpu"]'),
        { timeout: 5000 }
      );
    });

    await record('control node shows an override badge for the manually overridden control', async () => {
      await page.waitForSelector(
        `[data-node-id="${CONTROL_NODE}"] .row [data-node-override]`,
        { timeout: 5000 }
      );
    });

    await record('clean config shows live rows ungreyed and no node warning badges', async () => {
      await page.waitForFunction(
        (nodeId, label) => {
          const rows = Array.from(document.querySelectorAll(`[data-node-id="${nodeId}"] .row`));
          const row = rows.find((el) => el.textContent.includes(label));
          return !!row && !row.classList.contains('missing');
        },
        { timeout: 5000 },
        SENSOR_NODE,
        SENSOR_LABEL
      );
      const badges = await page.$$eval('[data-node-warning]', (els) => els.map((el) => el.getAttribute('title')));
      assert(badges.length === 0, `unexpected node warning badges on a clean config: ${badges.join(' | ')}`);
    });

    await record('device rows wired past the fold render their edges on load', async () => {
      await page.waitForSelector(`g.svelte-flow__edge[data-id="${LATE_SENSOR_EDGE_ID}"] path`, { timeout: 5000 });
      await page.waitForSelector(`g.svelte-flow__edge[data-id="${LATE_CONTROL_EDGE_ID}"] path`, { timeout: 5000 });
      assert(await page.$(handleSelector(SENSOR_NODE, TEMP5)), 'wired temp5 handle should be visible without expanding');
      assert(await page.$(handleSelector(CONTROL_NODE, PWM5)), 'wired pwm5 handle should be visible without expanding');
      assert(!(await page.$(handleSelector(SENSOR_NODE, TEMP3))), 'unwired temp3 should still be folded');
      assert(!(await page.$(handleSelector(CONTROL_NODE, PWM3))), 'unwired pwm3 should still be folded');
    });

    await record('expanding the sensor node shows fan1 as an RPM value', async () => {
      await page.evaluate((nodeId) => {
        document.querySelector(`[data-node-id="${nodeId}"] button.fold`).click();
      }, SENSOR_NODE);
      await page.waitForSelector(handleSelector(SENSOR_NODE, TEMP3), { timeout: 5000 });
      const fanText = await page.evaluate((nodeId) => {
        const rows = Array.from(document.querySelectorAll(`[data-node-id="${nodeId}"] .row`));
        const row = rows.find((el) => el.textContent.includes('fan1'));
        return row ? row.textContent : null;
      }, SENSOR_NODE);
      assert(fanText !== null, 'fan1 row not found');
      assert(fanText.includes('1200 RPM'), `fan1 row should read as RPM, got: ${fanText}`);
      assert(!fanText.includes(' C'), `fan1 row is still formatted as a temperature: ${fanText}`);
    });

    await record('virtual node exposes one more input handle than it has connections', async () => {
      await page.waitForSelector(handleSelector('virtual:combined', 'in-2'), { timeout: 5000 });
      const handles = await page.$$eval('[data-node-id="virtual:combined"] [data-handleid^="in-"]', (els) =>
        els.map((el) => el.getAttribute('data-handleid'))
      );
      deepStrictEqual(handles, ['in-0', 'in-1', 'in-2']);
    });

    await record('selecting the curve node opens its point editor in the panel', async () => {
      const clicked = await page.evaluate(() => {
        const node = document.querySelector('[data-node-id="curve:cpu"]');
        if (!node) return false;
        node.dispatchEvent(new MouseEvent('click', { bubbles: true }));
        return true;
      });
      assert(clicked, 'curve:cpu node not found');
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

    await record('unsaved-edits guard: cancel keeps edits and stays on Graph', async () => {
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
      const stillOnGraph = await page.$('.svelte-flow');
      assert(stillOnGraph, 'expected to remain on Graph after cancelling navigation');
      const pointsAfterNav = await countCurvePoints(page);
      assert(
        pointsAfterNav === pointsBeforeNav,
        `edits were lost across the cancelled navigation: ${pointsBeforeNav} -> ${pointsAfterNav}`
      );
    });

    await record('adding a max node wired from two sensors saves the expected TOML', async () => {
      const before = await virtualNodeIds(page);
      await page.evaluate(() => {
        document.querySelector('.gale-toolbar .add-node-menu > button').click();
      });
      await page.waitForSelector('[data-testid="add-node-virtual"]', { timeout: 5000 });
      await page.evaluate(() => {
        document.querySelector('[data-testid="add-node-virtual"]').click();
      });
      await page.waitForFunction(
        (n) => document.querySelectorAll('[data-node-id^="virtual:"]').length > n,
        { timeout: 5000 },
        before.length
      );
      const added = (await virtualNodeIds(page)).filter((id) => !before.includes(id));
      assert(added.length === 1, `expected exactly one new virtual node, got ${added.join(', ')}`);

      await page.waitForSelector('[data-testid="virtual-sensor-name"]', { timeout: 5000 });
      await page.evaluate((name) => {
        const input = document.querySelector('[data-testid="virtual-sensor-name"]');
        input.value = name;
        input.dispatchEvent(new Event('input', { bubbles: true }));
        input.dispatchEvent(new Event('change', { bubbles: true }));
        input.blur();
      }, NEW_SENSOR_NAME);
      const newNodeId = `virtual:${NEW_SENSOR_NAME}`;
      await page.waitForSelector(`[data-node-id="${newNodeId}"]`, { timeout: 5000 });
      assert(
        !(await page.$(`[data-node-id="${added[0]}"]`)),
        `renamed node still present under its old id ${added[0]}`
      );

      await page.waitForSelector(handleSelector(newNodeId, 'in-0'), { timeout: 5000 });
      await page.waitForSelector(`[data-node-id="${newNodeId}"] [data-node-warning]`, { timeout: 5000 });
      await fitView(page);
      assert(!(await page.$(handleSelector(newNodeId, 'in-1'))), 'in-1 should not exist before the first connection');
      await dragConnection(page, handleSelector(SENSOR_NODE, TEMP1), handleSelector(newNodeId, 'in-0'));
      await page.waitForSelector(handleSelector(newNodeId, 'in-1'), { timeout: 5000 });
      await dragConnection(page, handleSelector(SENSOR_NODE, TEMP2), handleSelector(newNodeId, 'in-1'));
      await page.waitForSelector(handleSelector(newNodeId, 'in-2'), { timeout: 5000 });

      await saveGraph(page);
      const config = await fetchConfig();
      deepStrictEqual(config.profiles.default.sensors[NEW_SENSOR_NAME], {
        type: 'max',
        inputs: [TEMP1, TEMP2],
      });
    });

    await record('wiring the new virtual node clears its warning badge', async () => {
      await page.waitForFunction(
        (nodeId) => !document.querySelector(`[data-node-id="${nodeId}"] [data-node-warning]`),
        { timeout: 5000 },
        `virtual:${NEW_SENSOR_NAME}`
      );
    });

    await record('deleting the control edge removes the assignment', async () => {
      const edgeSelector = `g.svelte-flow__edge[data-id="${CONTROL_EDGE_ID}"]`;
      await page.waitForSelector(edgeSelector, { timeout: 5000 });
      await page.evaluate((selector) => {
        if (document.activeElement) document.activeElement.blur();
        document.querySelector(selector).dispatchEvent(new MouseEvent('click', { bubbles: true }));
      }, edgeSelector);
      await page.waitForSelector(`${edgeSelector}.selected`, { timeout: 5000 });
      await page.keyboard.press('Backspace');
      await page.waitForFunction(
        (selector) => !document.querySelector(selector),
        { timeout: 5000 },
        edgeSelector
      );

      await saveGraph(page);
      const config = await fetchConfig();
      deepStrictEqual(config.profiles.default.assignments, { [PWM5]: 'late' });
    });

    await record('Save round-trips 204', async () => {
      await saveGraph(page);
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
