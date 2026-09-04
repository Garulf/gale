<script>
  import { untrack } from 'svelte';
  import PointCurveEditor from '../../components/PointCurveEditor.svelte';
  import { snapshot } from '../../store.js';
  import { getWebhookUrl } from '../../api.js';
  import { SENSOR_TYPES } from '../../sensors.js';
  import { CURVE_TYPES } from '../edit.js';
  import { tempValue, dutyValue, formatTemp, formatDuty, formatDeltaRate } from '../liveValues.js';
  import { shouldSnapshotEdit } from '../snapshotDebounce.js';

  let { node, edges, onUpdateData, onDeleteNode, onRenameNode, onRetypeNode } = $props();

  const FIELD_SNAPSHOT_DEBOUNCE_MS = 400;
  let lastFieldEditAt = null;

  function updateNodeField(id, updater) {
    const now = Date.now();
    const takeSnapshot = shouldSnapshotEdit(lastFieldEditAt, now, FIELD_SNAPSHOT_DEBOUNCE_MS);
    lastFieldEditAt = now;
    onUpdateData(id, updater, takeSnapshot);
  }

  const MIX_MODES = ['max', 'min', 'avg'];

  function newPointId() {
    return crypto.randomUUID();
  }

  function tagPoints(pairs) {
    return pairs.map(([temp, duty]) => ({ id: newPointId(), temp, duty }));
  }

  function untagPoints(tagged) {
    return tagged.map((point) => [point.temp, point.duty]);
  }

  function samePoints(a, b) {
    return a.length === b.length && a.every(([temp, duty], i) => temp === b[i][0] && duty === b[i][1]);
  }

  let taggedPoints = $state([]);

  $effect(() => {
    const isPointCurve = node && node.type === 'curve' && node.data.curve.config.type === 'point';
    const points = isPointCurve ? node.data.curve.config.points : [];
    if (!samePoints(points, untrack(() => untagPoints(taggedPoints)))) {
      taggedPoints = tagPoints(points);
    }
  });

  function nameOf(current) {
    if (!current) return '';
    if (current.type === 'virtual') return current.data.virtual.name;
    if (current.type === 'curve' || current.type === 'combine') return current.data[current.type].id;
    return '';
  }

  let nodeName = $derived(nameOf(node));
  let nameDraft = $state('');
  let nameError = $state('');

  $effect(() => {
    nameDraft = nodeName;
    nameError = '';
  });

  function commitName() {
    const name = nameDraft.trim();
    if (name === nodeName) {
      nameDraft = nodeName;
      nameError = '';
      return;
    }
    if (!name) {
      nameError = 'name must not be empty';
      return;
    }
    if (name.includes('/')) {
      nameError = 'name must not contain "/"';
      return;
    }
    nameError = onRenameNode(node.id, name);
  }

  function blurOnEnter(event) {
    if (event.key === 'Enter') event.target.blur();
  }

  function incomingEdge(targetHandle) {
    return (edges || []).find((edge) => edge.target === node.id && edge.targetHandle === targetHandle);
  }

  function outgoingEdge() {
    return (edges || []).find((edge) => edge.source === node.id && edge.sourceHandle === 'out');
  }

  let liveSensorTemp = $derived.by(() => {
    if (!node || node.type !== 'curve') return null;
    const edge = incomingEdge('sensor');
    if (!edge) return null;
    return tempValue($snapshot, edge.source, edge.sourceHandle);
  });

  let liveVirtualValue = $derived.by(() => {
    if (!node || node.type !== 'virtual') return null;
    return tempValue($snapshot, node.id, 'out');
  });

  let liveOutputDuty = $derived.by(() => {
    if (!node || (node.type !== 'curve' && node.type !== 'combine')) return null;
    const edge = outgoingEdge();
    if (!edge) return null;
    return dutyValue($snapshot, edge.targetHandle);
  });

  function updateCurveField(field, value) {
    updateNodeField(node.id, (data) => ({
      ...data,
      curve: { ...data.curve, config: { ...data.curve.config, [field]: value } },
    }));
  }

  function onPointsChange(newTaggedPoints) {
    taggedPoints = newTaggedPoints;
    updateCurveField('points', untagPoints(newTaggedPoints));
  }

  function toggleHysteresis(enabled) {
    updateCurveField('hysteresis', enabled ? { up: 2, down: 5 } : null);
  }

  function toggleResponse(enabled) {
    updateCurveField('response', enabled ? { rise_pct_per_sec: 10, fall_pct_per_sec: 10 } : null);
  }

  function updateHysteresisField(field, value) {
    updateCurveField('hysteresis', { ...node.data.curve.config.hysteresis, [field]: value });
  }

  function updateResponseField(field, value) {
    updateCurveField('response', { ...node.data.curve.config.response, [field]: value });
  }

  function updateCombineField(field, value) {
    updateNodeField(node.id, (data) => ({
      ...data,
      combine: { ...data.combine, config: { ...data.combine.config, [field]: value } },
    }));
  }

  function updateVirtualField(field, value) {
    updateNodeField(node.id, (data) => ({
      ...data,
      virtual: { ...data.virtual, config: { ...data.virtual.config, [field]: value } },
    }));
  }

  function toggleVirtualWindow(enabled) {
    updateVirtualField('window_s', enabled ? 10 : null);
  }

  let webhookUrl = $state('');
  let webhookUrlError = $state('');
  let copied = $state(false);

  let webhookKey = $derived.by(() => {
    if (!node || node.type !== 'virtual') return '';
    const config = node.data.virtual.config;
    if (config.type !== 'webhook' || !config.token) return '';
    return `${node.data.virtual.name}\n${config.token}`;
  });

  $effect(() => {
    const key = webhookKey;
    webhookUrl = '';
    webhookUrlError = '';
    copied = false;
    if (!key) return;
    const name = key.slice(0, key.indexOf('\n'));
    let cancelled = false;
    getWebhookUrl(name)
      .then((result) => {
        if (!cancelled) webhookUrl = result.url;
      })
      .catch((err) => {
        if (!cancelled) webhookUrlError = err.message;
      });
    return () => {
      cancelled = true;
    };
  });

  function toggleWebhookTimeout(enabled) {
    updateVirtualField('timeout_s', enabled ? 60 : null);
  }

  async function copyWebhookUrl() {
    try {
      await navigator.clipboard.writeText(webhookUrl);
      copied = true;
    } catch (err) {
      webhookUrlError = err.message;
    }
  }

  function numberFromEvent(event) {
    const raw = event.target.value.trim();
    if (raw === '') return null;
    const value = Number(raw);
    return Number.isNaN(value) ? null : value;
  }
</script>

{#snippet identity(types, testId)}
  <div class="identity">
    <label class="inline">
      Name
      <input
        type="text"
        data-testid={testId}
        bind:value={nameDraft}
        onchange={commitName}
        onkeydown={blurOnEnter}
      />
    </label>
    <label class="inline">
      Type
      <select
        data-testid="node-type"
        value={node.data[node.type].config.type}
        onchange={(e) => onRetypeNode(node.id, e.target.value)}
      >
        {#each types as type}
          <option value={type}>{type}</option>
        {/each}
      </select>
    </label>
    {#if nameError}
      <p class="error">{nameError}</p>
    {/if}
  </div>
{/snippet}

{#if !node}
  <p class="muted">No node selected.</p>
{:else if node.type === 'deviceSensor'}
  <h3>{node.data.deviceSensor.device} <small>device sensors</small></h3>
  <p class="muted">No editable fields. Use Hide on the node to remove it from the canvas.</p>
{:else if node.type === 'deviceControl'}
  <h3>{node.data.deviceControl.device} <small>device controls</small></h3>
  <p class="muted">No editable fields. Use Hide on the node to remove it from the canvas.</p>
{:else if node.type === 'virtual'}
  {@const config = node.data.virtual.config}
  <h3>{node.data.virtual.name} <small>virtual, {config.type}</small></h3>
  {@render identity(SENSOR_TYPES, 'virtual-sensor-name')}

  {#if config.type === 'offset'}
    <label class="inline">
      add
      <input
        type="number"
        value={config.add}
        oninput={(e) => updateVirtualField('add', numberFromEvent(e))}
      />
    </label>
    <label class="inline">
      scale
      <input
        type="number"
        value={config.scale}
        oninput={(e) => updateVirtualField('scale', numberFromEvent(e))}
      />
    </label>
  {:else if config.type === 'delta'}
    <label class="inline">
      window_s
      <input
        type="number"
        value={config.window_s}
        oninput={(e) => updateVirtualField('window_s', numberFromEvent(e))}
      />
    </label>
  {:else if config.type === 'mean'}
    <fieldset>
      <legend>
        <label>
          <input
            type="checkbox"
            checked={config.window_s != null}
            onchange={(e) => toggleVirtualWindow(e.target.checked)}
          />
          Moving average window
        </label>
      </legend>
      {#if config.window_s != null}
        <label class="inline">
          window_s
          <input
            type="number"
            value={config.window_s}
            oninput={(e) => updateVirtualField('window_s', numberFromEvent(e))}
          />
        </label>
      {/if}
    </fieldset>
  {:else if config.type === 'webhook'}
    <fieldset>
      <legend>
        <label>
          <input
            type="checkbox"
            data-testid="webhook-expires"
            checked={config.timeout_s != null}
            onchange={(e) => toggleWebhookTimeout(e.target.checked)}
          />
          Unavailable when no value arrives in time
        </label>
      </legend>
      {#if config.timeout_s != null}
        <label class="inline">
          timeout_s
          <input
            type="number"
            data-testid="webhook-timeout"
            value={config.timeout_s}
            oninput={(e) => updateVirtualField('timeout_s', numberFromEvent(e))}
          />
        </label>
      {/if}
    </fieldset>
    {#if !config.token}
      <p class="mini">Save the profile to generate this sensor's URL.</p>
    {:else if webhookUrlError}
      <p class="error">{webhookUrlError}</p>
    {:else}
      <label class="inline url">
        URL
        <input type="text" class="url" data-testid="webhook-url" readonly value={webhookUrl} />
      </label>
      <button type="button" class="copy" data-testid="webhook-copy" disabled={!webhookUrl} onclick={copyWebhookUrl}>
        {copied ? 'Copied' : 'Copy'}
      </button>
    {/if}
  {/if}

  <p class="mini">
    Live: {liveVirtualValue === null
      ? 'n/a'
      : (config.type === 'delta' ? formatDeltaRate : formatTemp)(liveVirtualValue)}
  </p>
  <div class="btns">
    <button type="button" onclick={() => onDeleteNode(node.id)}>Delete node</button>
  </div>
{:else if node.type === 'curve'}
  {@const config = node.data.curve.config}
  <h3>{node.data.curve.id} <small>{config.type}</small></h3>
  {@render identity(CURVE_TYPES, undefined)}

  {#if config.type === 'point'}
    <PointCurveEditor
      points={taggedPoints}
      liveTemp={liveSensorTemp}
      onChange={onPointsChange}
    />

    <fieldset>
      <legend>
        <label>
          <input
            type="checkbox"
            checked={config.hysteresis !== null}
            onchange={(e) => toggleHysteresis(e.target.checked)}
          />
          Hysteresis
        </label>
      </legend>
      {#if config.hysteresis}
        <label class="inline">
          up
          <input
            type="number"
            value={config.hysteresis.up}
            oninput={(e) => updateHysteresisField('up', numberFromEvent(e))}
          />
        </label>
        <label class="inline">
          down
          <input
            type="number"
            value={config.hysteresis.down}
            oninput={(e) => updateHysteresisField('down', numberFromEvent(e))}
          />
        </label>
      {/if}
    </fieldset>

    <fieldset>
      <legend>
        <label>
          <input
            type="checkbox"
            checked={config.response !== null}
            onchange={(e) => toggleResponse(e.target.checked)}
          />
          Response limiting
        </label>
      </legend>
      {#if config.response}
        <label class="inline">
          rise %/s
          <input
            type="number"
            value={config.response.rise_pct_per_sec}
            oninput={(e) => updateResponseField('rise_pct_per_sec', numberFromEvent(e))}
          />
        </label>
        <label class="inline">
          fall %/s
          <input
            type="number"
            value={config.response.fall_pct_per_sec}
            oninput={(e) => updateResponseField('fall_pct_per_sec', numberFromEvent(e))}
          />
        </label>
      {/if}
    </fieldset>
  {:else if config.type === 'flat'}
    <label class="inline">
      Duty %
      <input
        type="number"
        min="0"
        max="100"
        value={config.duty}
        oninput={(e) => updateCurveField('duty', numberFromEvent(e))}
      />
    </label>
  {:else if config.type === 'trigger'}
    <label class="inline">
      On temp
      <input
        type="number"
        value={config.on_temp}
        oninput={(e) => updateCurveField('on_temp', numberFromEvent(e))}
      />
    </label>
    <label class="inline">
      Off temp
      <input
        type="number"
        value={config.off_temp}
        oninput={(e) => updateCurveField('off_temp', numberFromEvent(e))}
      />
    </label>
    <label class="inline">
      On duty %
      <input
        type="number"
        value={config.on_duty}
        oninput={(e) => updateCurveField('on_duty', numberFromEvent(e))}
      />
    </label>
    <label class="inline">
      Off duty %
      <input
        type="number"
        value={config.off_duty}
        oninput={(e) => updateCurveField('off_duty', numberFromEvent(e))}
      />
    </label>
  {:else if config.type === 'target'}
    <label class="inline">
      Target temp
      <input
        type="number"
        value={config.target_temp}
        oninput={(e) => updateCurveField('target_temp', numberFromEvent(e))}
      />
    </label>
    <label class="inline">
      Step %/s
      <input
        type="number"
        value={config.step_pct_per_sec}
        oninput={(e) => updateCurveField('step_pct_per_sec', numberFromEvent(e))}
      />
    </label>
    <label class="inline">
      Min duty %
      <input
        type="number"
        value={config.min_duty}
        oninput={(e) => updateCurveField('min_duty', numberFromEvent(e))}
      />
    </label>
    <label class="inline">
      Max duty %
      <input
        type="number"
        value={config.max_duty}
        oninput={(e) => updateCurveField('max_duty', numberFromEvent(e))}
      />
    </label>
  {/if}

  <p class="mini">Live duty: {liveOutputDuty === null ? 'n/a' : formatDuty(liveOutputDuty)}</p>
  <div class="btns">
    <button type="button" onclick={() => onDeleteNode(node.id)}>Delete node</button>
  </div>
{:else if node.type === 'combine'}
  {@const config = node.data.combine.config}
  <h3>{node.data.combine.id} <small>{config.type}</small></h3>
  {@render identity(CURVE_TYPES, undefined)}

  {#if config.type === 'mix'}
    <label>
      Mode
      <select value={config.mode} onchange={(e) => updateCombineField('mode', e.target.value)}>
        {#each MIX_MODES as mode}
          <option value={mode}>{mode}</option>
        {/each}
      </select>
    </label>
  {/if}

  <p class="mini">Live duty: {liveOutputDuty === null ? 'n/a' : formatDuty(liveOutputDuty)}</p>
  <div class="btns">
    <button type="button" onclick={() => onDeleteNode(node.id)}>Delete node</button>
  </div>
{/if}

<style>
  .muted {
    opacity: 0.6;
  }

  .error {
    color: #f87171;
    font-size: 0.8rem;
    margin: 0.25rem 0 0;
  }

  .identity {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 0;
  }

  input[type='text'] {
    width: 9rem;
  }

  label.inline {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    margin-right: 1rem;
  }

  input,
  select {
    background: #14161a;
    color: inherit;
    border: 1px solid #2a3745;
    border-radius: 4px;
    padding: 0.25rem 0.4rem;
  }

  input[type='number'] {
    width: 5rem;
  }

  fieldset {
    border: 1px solid #2a3745;
    border-radius: 6px;
    margin: 0.75rem 0;
  }

  .mini {
    font-size: 0.8rem;
    opacity: 0.8;
  }

  .btns {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .btns button {
    background: none;
    border: 1px solid #2a3745;
    color: inherit;
    border-radius: 4px;
    padding: 0.35rem 0.8rem;
    cursor: pointer;
  }

  input.url {
    width: 100%;
    min-width: 18rem;
    font-family: ui-monospace, monospace;
    font-size: 0.8rem;
  }

  label.inline.url {
    display: flex;
    margin-right: 0;
  }

  button.copy {
    background: none;
    border: 1px solid #2a3745;
    color: inherit;
    border-radius: 4px;
    padding: 0.25rem 0.7rem;
    cursor: pointer;
    margin-top: 0.4rem;
  }
</style>
