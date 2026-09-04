<script>
  import { untrack } from 'svelte';
  import PointCurveEditor from '../../components/PointCurveEditor.svelte';
  import { snapshot } from '../../store.js';
  import { getWebhookUrl } from '../../api.js';
  import { isWebhookNotFound, webhookNameFor } from '../webhookPanel.js';
  import { SENSOR_TYPES } from '../../sensors.js';
  import { CURVE_TYPES } from '../edit.js';
  import { tempValue, dutyValue, formatDeltaRate } from '../liveValues.js';
  import { shouldSnapshotEdit } from '../snapshotDebounce.js';
  import { nodeKind } from '../ids.js';
  import { shortDevice } from '../../dashboard.js';

  let { node, nodes = [], edges, savedAt, onUpdateData, onDeleteNode, onRenameNode, onRetypeNode, onHideNode, onClose } = $props();

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

  function incomingEdges() {
    return (edges || []).filter((edge) => edge.target === node.id);
  }

  function incomingEdge(targetHandle) {
    return incomingEdges().find((edge) => edge.targetHandle === targetHandle);
  }

  function outgoingEdges() {
    return (edges || []).filter((edge) => edge.source === node.id && edge.sourceHandle === 'out');
  }

  function nodeById(id) {
    return nodes.find((candidate) => candidate.id === id);
  }

  function sourceLabel(edge) {
    const source = nodeById(edge.source);
    if (!source) return edge.source;
    if (source.type === 'deviceSensor') {
      const row = source.data.deviceSensor.rows.find((candidate) => candidate.handle === edge.sourceHandle);
      return row ? row.label : edge.sourceHandle;
    }
    return nameOf(source) || source.id;
  }

  function targetLabel(edge) {
    const target = nodeById(edge.target);
    if (!target) return edge.target;
    if (target.type === 'deviceControl') {
      const row = target.data.deviceControl.rows.find((candidate) => candidate.handle === edge.targetHandle);
      return row ? row.label : edge.targetHandle;
    }
    return nameOf(target) || target.id;
  }

  let liveSensorTemp = $derived.by(() => {
    if (!node || node.type !== 'curve') return null;
    const edge = incomingEdge('sensor');
    if (!edge) return null;
    return tempValue($snapshot, edge.source, edge.sourceHandle);
  });

  let sensorName = $derived.by(() => {
    if (!node || node.type !== 'curve') return '';
    const edge = incomingEdge('sensor');
    return edge ? sourceLabel(edge) : 'not wired';
  });

  let liveVirtualValue = $derived.by(() => {
    if (!node || node.type !== 'virtual') return null;
    return tempValue($snapshot, node.id, 'out');
  });

  let virtualInputs = $derived.by(() => {
    if (!node || node.type !== 'virtual') return [];
    return incomingEdges().map((edge) => ({ handle: edge.targetHandle, label: sourceLabel(edge), value: tempValue($snapshot, edge.source, edge.sourceHandle) }));
  });

  let combineSources = $derived.by(() => {
    if (!node || node.type !== 'combine') return [];
    return incomingEdges().map((edge) => sourceLabel(edge));
  });

  let outputTargets = $derived.by(() => {
    if (!node || (node.type !== 'curve' && node.type !== 'combine')) return [];
    return outgoingEdges().map((edge) => ({ label: targetLabel(edge), handle: edge.targetHandle, isControl: nodeKind(edge.target) === 'control' }));
  });

  let liveOutputDuty = $derived.by(() => {
    const control = outputTargets.find((target) => target.isControl);
    if (!control) return null;
    return dutyValue($snapshot, control.handle);
  });

  function fmt1(value) {
    return value === null || value === undefined ? '—' : value.toFixed(1);
  }

  function fmtInt(value) {
    return value === null || value === undefined ? '—' : String(Math.round(value));
  }

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
  let webhookNeedsSave = $state(false);
  let copied = $state(false);

  let webhookName = $derived.by(() => webhookNameFor(node));

  $effect(() => {
    const name = webhookName;
    void savedAt;
    webhookUrl = '';
    webhookUrlError = '';
    webhookNeedsSave = false;
    copied = false;
    if (!name) return;
    let cancelled = false;
    getWebhookUrl(name)
      .then((result) => {
        if (!cancelled) webhookUrl = result.url;
      })
      .catch((err) => {
        if (cancelled) return;
        if (isWebhookNotFound(err)) {
          webhookNeedsSave = true;
        } else {
          webhookUrlError = err.message;
        }
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

  let deviceRows = $derived.by(() => {
    if (!node) return [];
    if (node.type === 'deviceSensor') {
      return node.data.deviceSensor.rows.map((row) => {
        const value = tempValue($snapshot, node.id, row.handle);
        return { label: row.label, kind: row.kind || 'temp', text: value === null ? '—' : row.kind === 'rpm' ? `${Math.round(value)} rpm` : `${value.toFixed(1)} °C` };
      });
    }
    if (node.type === 'deviceControl') {
      return node.data.deviceControl.rows.map((row) => {
        const value = dutyValue($snapshot, row.handle);
        return { label: row.label, kind: 'duty', text: value === null ? '—' : `${Math.round(value)} %` };
      });
    }
    return [];
  });
</script>

{#snippet identity(kind, types, testId)}
  <div class="identity">
    <span class="kind {kind}"></span>
    <input
      type="text"
      class="name"
      aria-label="Node name"
      data-testid={testId}
      bind:value={nameDraft}
      onchange={commitName}
      onkeydown={blurOnEnter}
    />
    <select
      class="type"
      aria-label="Node type"
      data-testid="node-type"
      value={node.data[node.type].config.type}
      onchange={(e) => onRetypeNode(node.id, e.target.value)}
    >
      {#each types as type}
        <option value={type}>{type}</option>
      {/each}
    </select>
    {#if onClose}<button type="button" class="btn close" aria-label="Close" onclick={onClose}>×</button>{/if}
  </div>
  {#if nameError}
    <p class="error">{nameError}</p>
  {/if}
{/snippet}

{#snippet toggleBox(label, hint, checked, onToggle, content)}
  <div class="box">
    <label class="toggle">
      <input type="checkbox" {checked} onchange={(e) => onToggle(e.target.checked)} />
      <span class="toggle-label">{label}</span>
      <span class="toggle-hint">{hint}</span>
    </label>
    {#if checked}
      {@render content()}
    {/if}
  </div>
{/snippet}

{#if !node}
  <div class="empty">
    <span class="glyph">⌖</span>
    Select a node to edit it.
    <span class="sub">Drag from a temperature port into a curve, or from a duty port into a fan.</span>
  </div>
{:else if node.type === 'deviceSensor' || node.type === 'deviceControl'}
  {@const isSensor = node.type === 'deviceSensor'}
  {@const device = isSensor ? node.data.deviceSensor.device : node.data.deviceControl.device}
  <div class="identity">
    <div class="device-title">
      <span class="title">{shortDevice(device)}</span>
      <span class="mono sub">{device} · {isSensor ? 'sensors' : 'controls'}</span>
    </div>
    {#if onClose}<button type="button" class="btn close" aria-label="Close" onclick={onClose}>×</button>{/if}
  </div>
  <div class="list">
    {#each deviceRows as row}
      <div class="list-row"><span>{row.label}</span><span class="mono {row.kind}">{row.text}</span></div>
    {/each}
  </div>
  <p class="note">Hardware nodes have no editable fields. Channels appear here as ports; unwired channels are folded on the canvas.</p>
  <button type="button" class="btn" onclick={() => onHideNode(node.id)}>Hide from canvas</button>
{:else if node.type === 'virtual'}
  {@const config = node.data.virtual.config}
  {@render identity('temp', SENSOR_TYPES, 'virtual-sensor-name')}
  <div class="stat-card">
    <span class="eyebrow">Live value</span>
    <span class="big mono temp">{config.type === 'delta' ? (liveVirtualValue === null ? '—' : formatDeltaRate(liveVirtualValue)) : `${fmt1(liveVirtualValue)}°`}</span>
  </div>
  {#if config.type !== 'webhook'}
    <div class="group">
      <span class="eyebrow">Inputs</span>
      {#each virtualInputs as input (input.handle)}
        <div class="list-row"><span class="swatch temp"></span><span>{input.label}</span><span class="mono temp">{fmt1(input.value)}°</span></div>
      {/each}
      <span class="tip">{config.type === 'offset' || config.type === 'delta' ? 'Wire one temperature port into this node.' : 'Wire more temperature ports into this node to add inputs.'}</span>
    </div>
  {/if}
  {#if config.type === 'offset'}
    <div class="two">
      <label class="field">add<input type="number" value={config.add} oninput={(e) => updateVirtualField('add', numberFromEvent(e))} /></label>
      <label class="field">scale<input type="number" value={config.scale} oninput={(e) => updateVirtualField('scale', numberFromEvent(e))} /></label>
    </div>
  {:else if config.type === 'delta'}
    <label class="field">window s<input type="number" value={config.window_s} oninput={(e) => updateVirtualField('window_s', numberFromEvent(e))} /></label>
  {:else if config.type === 'mean'}
    {#snippet meanWindow()}
      <label class="field">seconds<input type="number" value={config.window_s} oninput={(e) => updateVirtualField('window_s', numberFromEvent(e))} /></label>
    {/snippet}
    {@render toggleBox('Moving average window', 'smooth spikes', config.window_s != null, toggleVirtualWindow, meanWindow)}
  {:else if config.type === 'webhook'}
    {#snippet webhookTimeout()}
      <label class="field">timeout s<input type="number" data-testid="webhook-timeout" value={config.timeout_s} oninput={(e) => updateVirtualField('timeout_s', numberFromEvent(e))} /></label>
    {/snippet}
    <div class="box">
      <label class="toggle">
        <input type="checkbox" data-testid="webhook-expires" checked={config.timeout_s != null} onchange={(e) => toggleWebhookTimeout(e.target.checked)} />
        <span class="toggle-label">Expire without a POST</span>
        <span class="toggle-hint">fail safe</span>
      </label>
      {#if config.timeout_s != null}
        {@render webhookTimeout()}
      {/if}
    </div>
    <div class="group">
      <span class="eyebrow">Webhook URL</span>
      {#if webhookNeedsSave}
        <p class="tip">Save the profile to generate this sensor's URL.</p>
      {:else if webhookUrlError}
        <p class="error">{webhookUrlError}</p>
      {:else}
        <div class="url-row">
          <input type="text" class="url mono" data-testid="webhook-url" readonly value={webhookUrl} aria-label="Webhook URL" />
          <button type="button" class="btn" data-testid="webhook-copy" disabled={!webhookUrl} onclick={copyWebhookUrl}>{copied ? 'Copied' : 'Copy'}</button>
        </div>
      {/if}
    </div>
  {/if}
  <button type="button" class="btn danger delete" onclick={() => onDeleteNode(node.id)}>Delete node</button>
{:else if node.type === 'curve'}
  {@const config = node.data.curve.config}
  {@render identity('duty', CURVE_TYPES, undefined)}
  <div class="two">
    <div class="stat-card">
      <span class="eyebrow">Input</span>
      <span class="mid mono temp">{config.type === 'flat' ? '—' : `${fmt1(liveSensorTemp)}°`}</span>
      <span class="sub">{config.type === 'flat' ? 'no sensor' : sensorName}</span>
    </div>
    <div class="stat-card">
      <span class="eyebrow">Output</span>
      <span class="mid mono duty">{fmtInt(liveOutputDuty)}%</span>
      <span class="sub">→ {outputTargets.map((target) => target.label).join(', ') || 'unassigned'}</span>
    </div>
  </div>

  {#if config.type === 'point'}
    <PointCurveEditor points={taggedPoints} liveTemp={liveSensorTemp} onChange={onPointsChange} />
    {#snippet hysteresisFields()}
      <div class="two">
        <label class="field">up<input type="number" value={config.hysteresis.up} oninput={(e) => updateHysteresisField('up', numberFromEvent(e))} />°</label>
        <label class="field">down<input type="number" value={config.hysteresis.down} oninput={(e) => updateHysteresisField('down', numberFromEvent(e))} />°</label>
      </div>
    {/snippet}
    {@render toggleBox('Hysteresis', 'avoid flutter', config.hysteresis !== null, toggleHysteresis, hysteresisFields)}
    {#snippet responseFields()}
      <div class="two">
        <label class="field">rise<input type="number" value={config.response.rise_pct_per_sec} oninput={(e) => updateResponseField('rise_pct_per_sec', numberFromEvent(e))} />%/s</label>
        <label class="field">fall<input type="number" value={config.response.fall_pct_per_sec} oninput={(e) => updateResponseField('fall_pct_per_sec', numberFromEvent(e))} />%/s</label>
      </div>
    {/snippet}
    {@render toggleBox('Response limiting', '%/s ramp', config.response !== null, toggleResponse, responseFields)}
  {:else if config.type === 'flat'}
    <label class="field">duty %<input type="number" min="0" max="100" value={config.duty} oninput={(e) => updateCurveField('duty', numberFromEvent(e))} /></label>
  {:else if config.type === 'trigger'}
    <div class="two">
      <label class="field">on °C<input type="number" value={config.on_temp} oninput={(e) => updateCurveField('on_temp', numberFromEvent(e))} /></label>
      <label class="field">on %<input type="number" value={config.on_duty} oninput={(e) => updateCurveField('on_duty', numberFromEvent(e))} /></label>
      <label class="field">off °C<input type="number" value={config.off_temp} oninput={(e) => updateCurveField('off_temp', numberFromEvent(e))} /></label>
      <label class="field">off %<input type="number" value={config.off_duty} oninput={(e) => updateCurveField('off_duty', numberFromEvent(e))} /></label>
    </div>
  {:else if config.type === 'target'}
    <div class="two">
      <label class="field">target °C<input type="number" value={config.target_temp} oninput={(e) => updateCurveField('target_temp', numberFromEvent(e))} /></label>
      <label class="field">step %/s<input type="number" value={config.step_pct_per_sec} oninput={(e) => updateCurveField('step_pct_per_sec', numberFromEvent(e))} /></label>
      <label class="field">min %<input type="number" value={config.min_duty} oninput={(e) => updateCurveField('min_duty', numberFromEvent(e))} /></label>
      <label class="field">max %<input type="number" value={config.max_duty} oninput={(e) => updateCurveField('max_duty', numberFromEvent(e))} /></label>
    </div>
  {/if}
  <button type="button" class="btn danger delete" onclick={() => onDeleteNode(node.id)}>Delete node</button>
{:else if node.type === 'combine'}
  {@const config = node.data.combine.config}
  {@render identity('duty', CURVE_TYPES, undefined)}
  <div class="stat-card">
    <span class="eyebrow">Output duty</span>
    <span class="big mono duty">{fmtInt(liveOutputDuty)}%</span>
    <span class="sub">{config.type === 'mix' ? `${config.mode} of ${combineSources.join(', ') || 'nothing yet'}` : `follows ${combineSources[0] || 'nothing yet'}`} → {outputTargets.map((target) => target.label).join(', ') || 'unassigned'}</span>
  </div>
  {#if config.type === 'mix'}
    <div class="group">
      <span class="eyebrow">Mode</span>
      <div class="segmented" role="radiogroup" aria-label="Mix mode">
        {#each MIX_MODES as mode}
          <button type="button" role="radio" aria-checked={config.mode === mode} class:on={config.mode === mode} onclick={() => updateCombineField('mode', mode)}>{mode}</button>
        {/each}
      </div>
    </div>
  {/if}
  <button type="button" class="btn danger delete" onclick={() => onDeleteNode(node.id)}>Delete node</button>
{/if}

<style>
  .empty {
    display: flex;
    flex-direction: column;
    gap: 8px;
    color: var(--muted);
    font-size: 13px;
    padding-top: 40px;
    text-align: center;
  }

  .glyph {
    font-size: 28px;
    color: var(--faint);
  }

  .sub {
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty .sub {
    color: var(--faint);
    white-space: normal;
  }

  .identity {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .kind {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .kind.temp {
    background: var(--temp);
  }

  .kind.duty {
    background: var(--duty);
  }

  .name {
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    border-bottom: 1px solid transparent;
    color: var(--ink);
    font-size: 18px;
    font-weight: var(--title-weight);
    padding: 2px 0;
    outline: none;
  }

  .name:focus {
    border-bottom-color: var(--accent);
  }

  .type {
    background: var(--surface2);
    border: 1px solid var(--line);
    color: var(--ink);
    border-radius: var(--r);
    padding: 5px 8px;
    font-size: 12px;
  }

  .close {
    width: 32px;
    height: 32px;
    padding: 0;
    font-size: 16px;
  }

  .device-title {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }

  .device-title .title {
    font-size: 18px;
    font-weight: var(--title-weight);
  }

  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .stat-card {
    border: 1px solid var(--line);
    border-radius: var(--r);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .mid {
    font-size: 16px;
  }

  .big {
    font-size: 22px;
  }

  .temp {
    color: var(--temp);
  }

  .duty {
    color: var(--duty);
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .list-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--surface2);
    border: 1px solid var(--line);
    border-radius: var(--r);
    font-size: 12.5px;
  }

  .list-row > span:nth-last-child(2),
  .list-row > span:first-child:not(.swatch) {
    flex: 1;
  }

  .swatch {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: 0 0 auto;
  }

  .swatch.temp {
    background: var(--temp);
  }

  .tip {
    font-size: 11px;
    color: var(--faint);
    margin: 0;
  }

  .note {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.5;
    margin: 0;
  }

  .box {
    border: 1px solid var(--line);
    border-radius: var(--r);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    cursor: pointer;
  }

  .toggle-label {
    flex: 1;
  }

  .toggle-hint {
    font-size: 11px;
    color: var(--faint);
  }

  .url-row {
    display: flex;
    gap: 6px;
  }

  .url {
    flex: 1;
    min-width: 0;
    background: var(--surface2);
    border: 1px solid var(--line);
    border-radius: var(--r);
    padding: 6px 8px;
    color: var(--ink);
    font-size: 11px;
  }

  .segmented {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    border: 1px solid var(--line);
    border-radius: var(--r);
    overflow: hidden;
  }

  .segmented button {
    padding: 8px;
    border: none;
    border-left: 1px solid var(--line);
    background: none;
    color: var(--muted);
    font-size: 12px;
  }

  .segmented button:first-child {
    border-left: none;
  }

  .segmented button.on {
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }

  .delete {
    margin-top: auto;
    padding: 8px;
  }

  @media (max-width: 720px) {
    .field {
      min-height: 44px;
    }

    .toggle {
      min-height: 32px;
    }

    .delete {
      padding: 12px;
    }
  }
</style>
