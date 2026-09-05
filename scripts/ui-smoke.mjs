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
const WEBHOOK_SENSOR_NAME = 'remote_temp';
const WEBHOOK_NODE = `virtual:${WEBHOOK_SENSOR_NAME}`;
const WEBHOOK_STATUS_ID = `virtual/${WEBHOOK_SENSOR_NAME}`;
const WEBHOOK_VALUE = 51.5;
const WEBHOOK_VALUE_TEXT = '51.5°C';
const NO_VALUE_TEXT = '—°C';
const WEBHOOK_TIMEOUT_S = 0.5;
const UNKNOWN_TOKEN = 'f'.repeat(64);

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
      const cards = Array.from(document.querySelectorAll('.temp-card'));
      return cards.some((card) => {
        const l = card.querySelector('.label');
        const v = card.querySelector('.value');
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

async function fetchStatus() {
  const response = await fetch(`${BASE_URL}/api/status`);
  assert(response.ok, `GET /api/status returned ${response.status}`);
  return response.json();
}

async function waitForStatusSensor(id, predicate, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    const status = await fetchStatus();
    last = status.sensors[id];
    if (predicate(last)) return last;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`status sensor ${id} never satisfied the predicate, last value ${JSON.stringify(last)}`);
}

async function postWebhook(token, value) {
  return fetch(`${BASE_URL}/api/webhook/${token}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ value }),
  });
}

async function fetchWebhookUrl(name) {
  const response = await fetch(`${BASE_URL}/api/webhook-url/${encodeURIComponent(name)}`);
  assert(response.ok, `GET /api/webhook-url/${name} returned ${response.status}`);
  const { url } = await response.json();
  return url;
}

async function setPanelInput(page, testId, value) {
  await page.evaluate(
    (testId, value) => {
      const input = document.querySelector(`[data-testid="${testId}"]`);
      input.value = value;
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new Event('change', { bubbles: true }));
    },
    testId,
    value
  );
}

async function addVirtualNode(page) {
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
  return added[0];
}

async function renameSelectedVirtualNode(page, name) {
  await setPanelInput(page, 'virtual-sensor-name', name);
  await page.evaluate(() => document.querySelector('[data-testid="virtual-sensor-name"]').blur());
  await page.waitForSelector(`[data-node-id="virtual:${name}"]`, { timeout: 5000 });
}

function nodeOutputSelector(nodeId) {
  return `[data-node-id="${nodeId}"] .row.out .val`;
}

async function webhookPanelState(page) {
  return page.evaluate(() => {
    const url = document.querySelector('[data-testid="webhook-url"]');
    const copy = document.querySelector('[data-testid="webhook-copy"]');
    const error = document.querySelector('.panel p.error, p.error');
    return {
      url: url ? url.value : null,
      copy: copy ? copy.textContent.trim() : null,
      error: error ? error.textContent.trim() : null,
    };
  });
}

async function waitForWebhookPanel(page, predicate, description) {
  const deadline = Date.now() + 5000;
  let state;
  while (Date.now() < deadline) {
    state = await webhookPanelState(page);
    if (predicate(state)) return state;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`${description}, panel state ${JSON.stringify(state)}`);
}

async function waitForNodeOutput(page, nodeId, text) {
  await page.waitForFunction(
    (selector, text) => {
      const el = document.querySelector(selector);
      return !!el && el.textContent.trim() === text;
    },
    { timeout: 5000 },
    nodeOutputSelector(nodeId),
    text
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
    await browser.defaultBrowserContext().overridePermissions(BASE_URL, ['clipboard-read', 'clipboard-sanitized-write']);
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

    await record('manual override badge appears within 3s of Hold', async () => {
      const card = await findControlCard(page, CONTROL_LABEL);
      const rangeInput = await card.$('input[type="range"]');
      await rangeInput.evaluate((el) => {
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
      assert(fanText.includes('1200rpm'), `fan1 row should read as RPM, got: ${fanText}`);
      assert(!fanText.includes('°C'), `fan1 row is still formatted as a temperature: ${fanText}`);
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

    await record('loading the Quiet preset onto the cpu curve saves its points', async () => {
      await page.waitForSelector('[data-testid="curve-preset"] option[value="Quiet"]', { timeout: 5000 });
      await setPanelInput(page, 'curve-preset', 'Quiet');
      await page.waitForFunction(() => document.querySelector('[data-testid="curve-preset"]').value === 'Quiet', { timeout: 5000 });
      await saveGraph(page);
      const config = await fetchConfig();
      deepStrictEqual(config.profiles.default.curves.cpu.points, [[30, 20], [50, 30], [65, 50], [75, 80], [85, 100]]);
    });

    await record('saving the cpu curve as a preset lists it under user presets and in the TOML', async () => {
      await page.evaluate(() => {
        window.prompt = () => 'smoke_preset';
      });
      await page.click('[data-testid="preset-save"]');
      await page.waitForFunction(() => document.querySelector('[data-testid="curve-preset"]').value === 'smoke_preset', { timeout: 5000 });
      const presets = await (await fetch(`${BASE_URL}/api/presets`)).json();
      assert(presets.user.smoke_preset && presets.user.smoke_preset.type === 'point', 'smoke_preset missing from GET /api/presets');
      const toml = await (await fetch(`${BASE_URL}/api/config.toml`)).text();
      assert(toml.includes('[presets.smoke_preset]'), 'TOML does not contain the saved preset');
      await page.waitForSelector('[data-testid="preset-delete"]', { timeout: 5000 });
    });

    await record('adding a max node wired from two sensors saves the expected TOML', async () => {
      const addedId = await addVirtualNode(page);
      await renameSelectedVirtualNode(page, NEW_SENSOR_NAME);
      const newNodeId = `virtual:${NEW_SENSOR_NAME}`;
      assert(
        !(await page.$(`[data-node-id="${addedId}"]`)),
        `renamed node still present under its old id ${addedId}`
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

    let webhookToken = '';
    await record('adding a webhook node saves with a daemon-generated token and no inputs', async () => {
      await addVirtualNode(page);
      await renameSelectedVirtualNode(page, WEBHOOK_SENSOR_NAME);
      await setPanelInput(page, 'node-type', 'webhook');
      await page.waitForFunction(
        (nodeId) =>
          document.querySelectorAll(`[data-node-id="${nodeId}"] [data-handleid^="in"]`).length === 0 &&
          !document.querySelector(`[data-node-id="${nodeId}"] [data-node-warning]`),
        { timeout: 5000 },
        WEBHOOK_NODE
      );
      await page.waitForSelector('[data-testid="webhook-expires"]', { timeout: 5000 });
      await waitForNodeOutput(page, WEBHOOK_NODE, NO_VALUE_TEXT);

      await saveGraph(page);
      const config = await fetchConfig();
      const sensor = config.profiles.default.sensors[WEBHOOK_SENSOR_NAME];
      assert(sensor && sensor.type === 'webhook', `expected a webhook sensor, got ${JSON.stringify(sensor)}`);
      assert(sensor.token === '', `GET /api/config should redact the webhook token, got ${JSON.stringify(sensor.token)}`);
      assert(!('timeout_s' in sensor), `timeout_s should be absent, got ${JSON.stringify(sensor)}`);
      assert(!('inputs' in sensor) && !('input' in sensor), `webhook sensor should carry no inputs: ${JSON.stringify(sensor)}`);

      const url = await fetchWebhookUrl(WEBHOOK_SENSOR_NAME);
      const match = /\/api\/webhook\/([0-9a-f]{64})$/.exec(url);
      assert(match, `webhook URL is not shaped as expected: ${url}`);
      webhookToken = match[1];
    });

    await record('the panel shows the saved webhook URL and the copy button puts it on the clipboard', async () => {
      const expected = `${BASE_URL}/api/webhook/${webhookToken}`;
      await waitForWebhookPanel(page, (state) => state.url === expected, `panel never showed ${expected}`);
      await page.evaluate(() => document.querySelector('[data-testid="webhook-copy"]').click());
      await waitForWebhookPanel(page, (state) => state.copy === 'Copied', 'copy button never read Copied');
      const clipboard = await page.evaluate(() => navigator.clipboard.readText());
      assert(clipboard === expected, `clipboard holds ${clipboard}, expected ${expected}`);

      await saveGraph(page);
      const config = await fetchConfig();
      assert(
        config.profiles.default.sensors[WEBHOOK_SENSOR_NAME].token === '',
        'GET /api/config should redact the webhook token'
      );

      const response = await postWebhook(webhookToken, WEBHOOK_VALUE);
      assert(
        response.status === 204,
        `posting to the original token after a second save should still work (no rotation), got ${response.status}`
      );
    });

    await record('reloading the page after a save still shows the working webhook URL', async () => {
      const expected = `${BASE_URL}/api/webhook/${webhookToken}`;
      await page.reload({ waitUntil: 'networkidle0' });
      await page.waitForSelector('h2', { timeout: 5000 });

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

      await page.waitForSelector(`[data-node-id="${WEBHOOK_NODE}"]`, { timeout: 5000 });
      const nodeClicked = await page.evaluate((nodeId) => {
        const node = document.querySelector(`[data-node-id="${nodeId}"]`);
        if (!node) return false;
        node.dispatchEvent(new MouseEvent('click', { bubbles: true }));
        return true;
      }, WEBHOOK_NODE);
      assert(nodeClicked, `${WEBHOOK_NODE} not found after reload`);

      await waitForWebhookPanel(page, (state) => state.url === expected, `panel never showed ${expected} after reload`);
    });

    await record('POSTing a value to the webhook URL shows on the node and in status', async () => {
      const response = await postWebhook(webhookToken, WEBHOOK_VALUE);
      assert(response.status === 204, `expected 204 from the webhook, got ${response.status}`);
      await waitForStatusSensor(WEBHOOK_STATUS_ID, (v) => v === WEBHOOK_VALUE, 5000);
      await waitForNodeOutput(page, WEBHOOK_NODE, WEBHOOK_VALUE_TEXT);
      const wrongToken = await postWebhook(UNKNOWN_TOKEN, 1);
      assert(wrongToken.status === 404, `unknown token should be 404, got ${wrongToken.status}`);
    });

    await record('a webhook value expires after timeout_s and the node reads n/a again', async () => {
      await page.evaluate(() => document.querySelector('[data-testid="webhook-expires"]').click());
      await page.waitForSelector('[data-testid="webhook-timeout"]', { timeout: 5000 });
      await setPanelInput(page, 'webhook-timeout', String(WEBHOOK_TIMEOUT_S));
      await saveGraph(page);
      const config = await fetchConfig();
      assert(
        config.profiles.default.sensors[WEBHOOK_SENSOR_NAME].timeout_s === WEBHOOK_TIMEOUT_S,
        `timeout_s did not save: ${JSON.stringify(config.profiles.default.sensors[WEBHOOK_SENSOR_NAME])}`
      );

      const response = await postWebhook(webhookToken, WEBHOOK_VALUE);
      assert(response.status === 204, `expected 204 from the webhook, got ${response.status}`);
      await waitForStatusSensor(WEBHOOK_STATUS_ID, (v) => v === WEBHOOK_VALUE, 5000);
      await waitForStatusSensor(WEBHOOK_STATUS_ID, (v) => v === null, 5000);
      await waitForNodeOutput(page, WEBHOOK_NODE, NO_VALUE_TEXT);
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
