<script>
  import { getContext, untrack, onDestroy } from 'svelte';
  import { curvePoints as curvePointsFor } from '../../curveMath.js';
  import PointCurveEditor from '../../components/PointCurveEditor.svelte';
  import { snapshot } from '../../store.js';
  import { getWebhookUrl, putControlSettings, startCalibration, getCalibration, cancelCalibration } from '../../api.js';
  import { daemonConfig, refreshConfig } from '../../config.js';
  import { isWebhookNotFound, webhookNameFor, maskWebhookUrl, webhookSensorNodes } from '../webhookPanel.js';
  import { SENSOR_TYPES } from '../../sensors.js';
  import { CURVE_TYPES } from '../edit.js';
  import { tempValue, dutyValue, formatDeltaRate, sensorDisplay } from '../liveValues.js';
  import { shouldSnapshotEdit } from '../snapshotDebounce.js';
  import { nodeKind } from '../ids.js';
  import { shortDevice } from '../../dashboard.js';
  import { presets, savePreset, removePreset, refreshPresets, presetNameFor } from '../../presets.js';

  let { node, nodes = [], edges, savedAt, onUpdateData, onDeleteNode, onRenameNode, onRetypeNode, onApplyPreset, onDuplicateNode, onHideNode, onClose, onRenameRow, onToggleRow, onToggleCompact, groups = [], onRenameGroup, onUngroup, onEnterGroup, onSetMembership, onSelectNode } = $props();

  const nodeWarnings = getContext('galeNodeWarnings');

  function memberName(memberId) {
    return memberId.slice(memberId.indexOf(':') + 1);
  }

  function groupMemberWarnings(members) {
    if (!nodeWarnings) return [];
    const all = nodeWarnings();
    return members.flatMap((member) => (all[member] || []).map((message) => `${memberName(member)}: ${message}`));
  }

  const FIELD_SNAPSHOT_DEBOUNCE_MS = 400;
  let lastFieldEditAt = null;

  function updateNodeField(id, updater) {
    const now = Date.now();
    const takeSnapshot = shouldSnapshotEdit(lastFieldEditAt, now, FIELD_SNAPSHOT_DEBOUNCE_MS);
    lastFieldEditAt = now;
    onUpdateData(id, updater, takeSnapshot);
  }

  const MIX_MODES = ['max', 'min', 'avg', 'sum', 'subtract'];

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
  let groupNameError = $state('');

  let rowLabelError = $state('');

  async function commitRowLabel(handle, value) {
    rowLabelError = '';
    try {
      await onRenameRow(node.id, handle, value.trim());
    } catch (err) {
      rowLabelError = err.message;
    }
  }

  const CONTROL_LIMIT_FIELDS = [
    ['min_duty', 'min %', 'never below'],
    ['start_duty', 'start %', 'kick from stopped'],
    ['stop_duty', 'stop %', 'snap to 0 below'],
    ['max_duty', 'max %', 'never above'],
  ];
  let controlLimitsError = $state('');

  function controlSettingsFor(handle) {
    const config = $daemonConfig;
    return (config && config.controls && config.controls[handle]) || {};
  }

  async function commitControlLimit(handle, field, raw) {
    const trimmed = String(raw).trim();
    const value = trimmed === '' ? null : Number(trimmed);
    if (value !== null && Number.isNaN(value)) return;
    const next = { ...controlSettingsFor(handle), [field]: value };
    for (const key of Object.keys(next)) if (next[key] === null || next[key] === undefined) delete next[key];
    controlLimitsError = '';
    try {
      await putControlSettings(handle, next);
      await refreshConfig();
    } catch (err) {
      controlLimitsError = err.message;
    }
  }

  const CALIBRATION_POLL_MS = 1500;
  let calibration = $state(null);
  let calibrationError = $state('');
  let calibrationAborting = $state(false);
  let calibrationTimer = null;

  function stopCalibrationPolling() {
    if (calibrationTimer) clearInterval(calibrationTimer);
    calibrationTimer = null;
  }

  function startCalibrationPolling(handle) {
    stopCalibrationPolling();
    calibrationTimer = setInterval(() => pollCalibration(handle), CALIBRATION_POLL_MS);
  }

  async function pollCalibration(handle) {
    try {
      const progress = await getCalibration(handle);
      calibration = progress && progress.state !== 'idle' ? progress : null;
      if (!progress || progress.state !== 'running') {
        stopCalibrationPolling();
        calibrationAborting = false;
        if (progress && progress.state === 'done') await refreshConfig();
      }
    } catch (err) {
      calibrationError = err.message;
      calibrationAborting = false;
      stopCalibrationPolling();
    }
  }

  async function detectLimits(handle) {
    calibrationError = '';
    calibrationAborting = false;
    try {
      await startCalibration(handle);
      calibration = { state: 'running', control: handle, phase: 'probe', duty: 50, rpm: null };
      startCalibrationPolling(handle);
    } catch (err) {
      calibrationError = err.message;
    }
  }

  async function abortCalibration(handle) {
    calibrationAborting = true;
    try {
      await cancelCalibration(handle);
    } catch (err) {
      calibrationError = err.message;
      calibrationAborting = false;
    }
  }

  function calibrationText(progress) {
    if (progress.state === 'running') {
      const rpm = progress.rpm === null || progress.rpm === undefined ? 'waiting for rpm' : `${Math.round(progress.rpm)} rpm`;
      return `Detecting (${progress.phase}) at ${Math.round(progress.duty)} %: ${rpm}`;
    }
    if (progress.state === 'done') {
      const r = progress.result;
      return `Detected min ${r.min_duty} %, start ${r.start_duty} %, stop ${r.stop_duty} %`;
    }
    if (progress.state === 'cancelled') return 'Detection cancelled';
    return `Detection failed: ${progress.error}`;
  }

  let controlHandles = $derived(
    node && node.type === 'deviceControl' ? node.data.deviceControl.rows.map((row) => row.handle).join('\n') : ''
  );

  $effect(() => {
    const handles = controlHandles;
    calibrationError = '';
    if (untrack(() => calibration)?.state !== 'running') {
      stopCalibrationPolling();
      calibration = null;
      calibrationAborting = false;
    }
    if (!handles) return;
    let dropped = false;
    (async () => {
      for (const handle of handles.split('\n')) {
        if (dropped || untrack(() => calibration)) return;
        let progress;
        try {
          progress = await getCalibration(handle);
        } catch {
          return;
        }
        if (dropped) return;
        if (progress && progress.state === 'running') {
          calibration = progress;
          startCalibrationPolling(progress.control);
          return;
        }
      }
    })();
    return () => {
      dropped = true;
    };
  });

  onDestroy(stopCalibrationPolling);

  let presetName = $derived(node && node.type === 'curve' ? presetNameFor(node.data.curve.config, $presets) : '');
  let presetError = $state('');
  let isSavedPreset = $derived(presetName !== '' && presetName in $presets.user);

  function loadPreset(name) {
    const preset = $presets.builtin[name] || $presets.user[name];
    if (!preset) return;
    presetError = '';
    onApplyPreset(node.id, preset);
  }

  async function saveAsPreset() {
    const name = (window.prompt('Save preset as', isSavedPreset ? presetName : '') || '').trim();
    if (!name) return;
    if (name in $presets.user && !window.confirm(`Overwrite preset "${name}"?`)) return;
    presetError = '';
    try {
      await savePreset(name, node.data.curve.config);
    } catch (err) {
      presetError = err.message;
    }
  }

  async function deleteSavedPreset() {
    if (!window.confirm(`Delete preset "${presetName}"?`)) return;
    try {
      await removePreset(presetName);
      presetError = '';
    } catch (err) {
      if (err.message.startsWith('404')) {
        presetError = '';
        await refreshPresets();
      } else {
        presetError = err.message;
      }
    }
  }

  $effect(() => {
    void node?.id;
    nameDraft = nodeName;
    nameError = '';
    groupNameError = '';
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

  function optionalNumberFromEvent(event) {
    const raw = event.target.value.trim();
    if (raw === '') return null;
    const value = Number(raw);
    return Number.isNaN(value) ? null : value;
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
  let webhookRevealed = $state(false);

  let webhookName = $derived.by(() => webhookNameFor(node));
  let registeredWebhooks = $derived(webhookSensorNodes(nodes));

  $effect(() => {
    const name = webhookName;
    void savedAt;
    webhookUrl = '';
    webhookUrlError = '';
    webhookNeedsSave = false;
    copied = false;
    webhookRevealed = false;
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
        const reading = sensorDisplay(tempValue($snapshot, node.id, row.handle), row.kind);
        return { handle: row.handle, label: row.label, kind: row.kind || 'temp', text: `${reading.text} ${reading.unit}`, hidden: row.hidden === true, wired: row.wired === true };
      });
    }
    if (node.type === 'deviceControl') {
      return node.data.deviceControl.rows.map((row) => {
        const value = dutyValue($snapshot, row.handle);
        return { handle: row.handle, label: row.label, kind: 'duty', text: value === null ? '—' : `${Math.round(value)} %`, hidden: row.hidden === true, wired: row.wired === true };
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

{#snippet nodeActions()}
  <div class="actions">
    <button type="button" class="btn" data-testid="node-duplicate" onclick={() => onDuplicateNode(node.id)}>Duplicate</button>
    <button type="button" class="btn danger delete" onclick={() => onDeleteNode(node.id)}>Delete node</button>
  </div>
{/snippet}

{#snippet membership()}
  <label class="field">
    <span>Group</span>
    <select data-testid="node-group" value={groups.find((group) => group.members.includes(node.id))?.id || ''} onchange={(e) => onSetMembership(node.id, e.target.value)}>
      <option value="">None</option>
      {#each groups as group (group.id)}
        <option value={group.id}>{group.name}</option>
      {/each}
    </select>
  </label>
{/snippet}

{#snippet smoothing(withHysteresis)}
  {@const config = node.data.curve.config}
  {#snippet hysteresisFields()}
    <div class="two">
      <label class="field">up<input type="number" value={config.hysteresis.up} oninput={(e) => updateHysteresisField('up', numberFromEvent(e))} />°</label>
      <label class="field">down<input type="number" value={config.hysteresis.down} oninput={(e) => updateHysteresisField('down', numberFromEvent(e))} />°</label>
    </div>
  {/snippet}
  {#snippet responseFields()}
    <div class="two">
      <label class="field">rise<input type="number" value={config.response.rise_pct_per_sec} oninput={(e) => updateResponseField('rise_pct_per_sec', numberFromEvent(e))} />%/s</label>
      <label class="field">fall<input type="number" value={config.response.fall_pct_per_sec} oninput={(e) => updateResponseField('fall_pct_per_sec', numberFromEvent(e))} />%/s</label>
    </div>
  {/snippet}
  {#if withHysteresis}
    {@render toggleBox('Hysteresis', 'avoid flutter', config.hysteresis !== null, toggleHysteresis, hysteresisFields)}
  {/if}
  {@render toggleBox('Response limiting', '%/s ramp', config.response !== null, toggleResponse, responseFields)}
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
    {#each deviceRows as row (row.handle)}
      <div class="list-row" class:row-hidden={row.hidden}>
        <button
          type="button"
          class="eye"
          data-testid="row-visibility"
          data-handle={row.handle}
          aria-pressed={!row.hidden}
          aria-label={row.hidden ? `Show ${row.label} on the graph` : `Hide ${row.label} from the graph`}
          title={row.hidden ? (row.wired ? 'Hidden, but shown while wired' : 'Hidden from the graph') : 'Shown on the graph'}
          onclick={() => onToggleRow(node.id, row.handle)}
        >
          {#if row.hidden}
            <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path d="M3 3l18 18M10.6 10.6a2 2 0 0 0 2.8 2.8M9.9 5.1A10.4 10.4 0 0 1 12 5c5 0 9 4 10 7a11.4 11.4 0 0 1-3.2 4.2M6.2 6.2A11.6 11.6 0 0 0 2 12c1 3 5 7 10 7a9.9 9.9 0 0 0 4.1-.9" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
          {:else}
            <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path d="M2 12c1-3 5-7 10-7s9 4 10 7c-1 3-5 7-10 7S3 15 2 12z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" /><circle cx="12" cy="12" r="3" fill="none" stroke="currentColor" stroke-width="1.8" /></svg>
          {/if}
        </button>
        <input
          type="text"
          class="row-label"
          aria-label="Label for {row.handle}"
          data-testid="row-label"
          data-handle={row.handle}
          value={row.label}
          onchange={(e) => commitRowLabel(row.handle, e.target.value)}
          onkeydown={blurOnEnter}
        />
        <span class="mono {row.kind}">{row.text}</span>
      </div>
      {#if !isSensor}
        {@const limits = controlSettingsFor(row.handle)}
        <div class="limits">
          {#each CONTROL_LIMIT_FIELDS as [field, label, hint] (field)}
            <label class="limit" title={hint}>
              <span>{label}</span>
              <input
                type="number"
                min="0"
                max="100"
                placeholder="—"
                data-testid="control-{field}"
                data-handle={row.handle}
                value={limits[field] ?? ''}
                onchange={(e) => commitControlLimit(row.handle, field, e.target.value)}
                onkeydown={blurOnEnter}
              />
            </label>
          {/each}
        </div>
        <div class="calibrate">
          {#if calibration && calibration.control === row.handle && calibration.state === 'running'}
            <span class="note">{calibrationText(calibration)}</span>
            <button type="button" class="btn" data-testid="control-calibrate-abort" disabled={calibrationAborting} onclick={() => abortCalibration(row.handle)}>{calibrationAborting ? 'Aborting…' : 'Abort'}</button>
          {:else}
            <button type="button" class="btn" data-testid="control-calibrate" data-handle={row.handle} disabled={calibration && calibration.state === 'running'} title="Sweeps the duty down until the fan stalls and back up until it restarts, then fills min, start and stop" onclick={() => detectLimits(row.handle)}>Detect limits</button>
            {#if calibration && calibration.control === row.handle}
              <span class="note" class:error={calibration.state === 'failed'}>{calibrationText(calibration)}</span>
            {/if}
          {/if}
        </div>
      {/if}
    {/each}
  </div>
  {#if calibrationError}
    <p class="error">{calibrationError}</p>
  {/if}
  {#if rowLabelError}
    <p class="error">{rowLabelError}</p>
  {/if}
  {#if controlLimitsError}
    <p class="error">{controlLimitsError}</p>
  {/if}
  {#if isSensor}
    <p class="note">Rename a channel by editing its label; clear it to restore the hardware name. Labels apply everywhere immediately and are kept in config.toml. The eye hides a channel from this profile's canvas (saved with the profile; wired channels stay visible). Unwired channels are folded on the canvas.</p>
  {:else}
    <p class="note">Rename a channel by editing its label; clear it to restore the hardware name. Limits apply to whatever curve drives the channel: below stop the fan snaps to 0, start kicks it from a stop, min is a hard floor, max is a hard ceiling. Both save immediately to config.toml.</p>
  {/if}
  {@render membership()}
  <button type="button" class="btn" onclick={() => onHideNode(node.id)}>Hide from canvas</button>
{:else if node.type === 'group'}
  {@const group = node.data.group}
  {@const memberWarnings = groupMemberWarnings(group.members)}
  <div class="identity">
    <span class="kind group"></span>
    <input
      type="text"
      class="name"
      aria-label="Group name"
      data-testid="group-name"
      value={group.name}
      onchange={(e) => (groupNameError = onRenameGroup(group.id, e.target.value))}
      onkeydown={blurOnEnter}
    />
    {#if onClose}<button type="button" class="btn close" aria-label="Close" onclick={onClose}>×</button>{/if}
  </div>
  {#if groupNameError}<p class="error">{groupNameError}</p>{/if}
  <p class="note">{group.members.length} nodes. Boundary connections appear as ports inside the group.</p>
  <ul class="members" data-testid="group-members">
    {#each group.members as member (member)}
      <li>
        <span class="mono">{memberName(member)}</span>
        <button type="button" class="btn" onclick={() => onSetMembership(member, '')}>Remove</button>
      </li>
    {/each}
  </ul>
  {#if memberWarnings.length > 0}
    <ul class="save-warnings" data-testid="group-warnings">
      {#each memberWarnings as warning}
        <li>{warning}</li>
      {/each}
    </ul>
  {/if}
  <div class="actions">
    <button type="button" class="btn" data-testid="group-enter" onclick={() => onEnterGroup(group.id)}>Open</button>
    <button type="button" class="btn danger delete" data-testid="group-ungroup" onclick={() => onUngroup(group.id)}>Ungroup</button>
  </div>
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
      <span class="tip">{config.type === 'offset' || config.type === 'delta' ? 'Wire one temperature port into this node.' : config.type === 'subtract' ? 'The first input is the base; every further input is subtracted from it.' : 'Wire more temperature ports into this node to add inputs.'}</span>
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
    {#if registeredWebhooks.length > 1 && onSelectNode}
      <label class="field">
        <span>Registered webhooks</span>
        <select data-testid="webhook-registered" value={node.id} onchange={(e) => onSelectNode(e.target.value)}>
          {#each registeredWebhooks as hook (hook.id)}
            <option value={hook.id}>{hook.name}</option>
          {/each}
        </select>
      </label>
    {/if}
    <div class="group">
      <span class="eyebrow">Webhook URL</span>
      {#if webhookNeedsSave}
        <p class="tip">Save the profile to generate this sensor's URL.</p>
      {:else if webhookUrlError}
        <p class="error">{webhookUrlError}</p>
      {:else}
        <div class="url-row">
          <input type="text" class="url mono" data-testid="webhook-url" readonly value={webhookRevealed ? webhookUrl : maskWebhookUrl(webhookUrl)} aria-label="Webhook URL" />
          <button type="button" class="btn" data-testid="webhook-reveal" disabled={!webhookUrl} onclick={() => (webhookRevealed = !webhookRevealed)}>{webhookRevealed ? 'Hide' : 'Reveal'}</button>
          <button type="button" class="btn" data-testid="webhook-copy" disabled={!webhookUrl} onclick={copyWebhookUrl}>{copied ? 'Copied' : 'Copy'}</button>
        </div>
      {/if}
    </div>
  {/if}
  {@render membership()}
  {@render nodeActions()}
{:else if node.type === 'curve'}
  {@const config = node.data.curve.config}
  {@render identity('duty', CURVE_TYPES, undefined)}
  <div class="preset-row">
    <select class="type" aria-label="Curve preset" data-testid="curve-preset" value={presetName} onchange={(e) => loadPreset(e.target.value)}>
      <option value="">custom shape</option>
      <optgroup label="Built-in">
        {#each Object.keys($presets.builtin) as name (name)}
          <option value={name}>{name}</option>
        {/each}
      </optgroup>
      {#if Object.keys($presets.user).length > 0}
        <optgroup label="Saved">
          {#each Object.keys($presets.user).sort() as name (name)}
            <option value={name}>{name}</option>
          {/each}
        </optgroup>
      {/if}
    </select>
    <button type="button" class="btn" data-testid="preset-save" onclick={saveAsPreset}>Save as</button>
    {#if isSavedPreset}
      <button type="button" class="btn danger" data-testid="preset-delete" onclick={deleteSavedPreset}>Delete</button>
    {/if}
  </div>
  {#if presetError}
    <p class="error">{presetError}</p>
  {/if}
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
    {@render smoothing(true)}
  {:else if config.type === 'linear'}
    <div class="two">
      <label class="field">from °C<input type="number" value={config.min_temp} oninput={(e) => updateCurveField('min_temp', numberFromEvent(e))} /></label>
      <label class="field">at %<input type="number" min="0" max="100" value={config.min_duty} oninput={(e) => updateCurveField('min_duty', numberFromEvent(e))} /></label>
      <label class="field">to °C<input type="number" value={config.max_temp} oninput={(e) => updateCurveField('max_temp', numberFromEvent(e))} /></label>
      <label class="field">at %<input type="number" min="0" max="100" value={config.max_duty} oninput={(e) => updateCurveField('max_duty', numberFromEvent(e))} /></label>
    </div>
    {@render smoothing(true)}
  {:else if config.type === 'flat'}
    <div class="slider-field">
      <input type="range" min="0" max="100" step="1" data-testid="flat-slider" aria-label="Duty" value={config.duty} oninput={(e) => updateCurveField('duty', numberFromEvent(e))} />
      <label class="field">duty %<input type="number" min="0" max="100" data-testid="flat-duty" value={config.duty} oninput={(e) => updateCurveField('duty', numberFromEvent(e))} /></label>
    </div>
  {:else if config.type === 'trigger'}
    <div class="two">
      <label class="field">on °C<input type="number" value={config.on_temp} oninput={(e) => updateCurveField('on_temp', numberFromEvent(e))} /></label>
      <label class="field">on %<input type="number" value={config.on_duty} oninput={(e) => updateCurveField('on_duty', numberFromEvent(e))} /></label>
      <label class="field">off °C<input type="number" value={config.off_temp} oninput={(e) => updateCurveField('off_temp', numberFromEvent(e))} /></label>
      <label class="field">off %<input type="number" value={config.off_duty} oninput={(e) => updateCurveField('off_duty', numberFromEvent(e))} /></label>
    </div>
    {@render smoothing(false)}
  {:else if config.type === 'target'}
    <div class="two">
      <label class="field">target °C<input type="number" value={config.target_temp} oninput={(e) => updateCurveField('target_temp', numberFromEvent(e))} /></label>
      <label class="field">step %/s<input type="number" value={config.step_pct_per_sec} oninput={(e) => updateCurveField('step_pct_per_sec', numberFromEvent(e))} /></label>
      <label class="field">min %<input type="number" value={config.min_duty} oninput={(e) => updateCurveField('min_duty', numberFromEvent(e))} /></label>
      <label class="field">max %<input type="number" value={config.max_duty} oninput={(e) => updateCurveField('max_duty', numberFromEvent(e))} /></label>
    </div>
    <div class="two">
      <label class="field" title="hold the duty while the temperature is within this many degrees of the target">deadband °<input type="number" min="0" step="0.5" placeholder="0.5" value={config.deadband ?? ''} oninput={(e) => updateCurveField('deadband', optionalNumberFromEvent(e))} /></label>
      <label class="field" title="at or below this temperature drop straight to min duty">idle °C<input type="number" placeholder="off" value={config.idle_temp ?? ''} oninput={(e) => updateCurveField('idle_temp', optionalNumberFromEvent(e))} /></label>
    </div>
  {/if}
  {#if config.type === 'flat' || curvePointsFor(config)}
    <label class="toggle compact-toggle">
      <input type="checkbox" data-testid="node-controls-toggle" checked={node.compact !== true} onchange={() => onToggleCompact(node.id)} />
      <span class="toggle-label">Show {config.type === 'flat' ? 'the slider' : 'the chart'} on the node</span>
    </label>
  {/if}
  {@render membership()}
  {@render nodeActions()}
{:else if node.type === 'combine'}
  {@const config = node.data.combine.config}
  {@render identity('duty', CURVE_TYPES, undefined)}
  <div class="stat-card">
    <span class="eyebrow">Output duty</span>
    <span class="big mono duty">{fmtInt(liveOutputDuty)}%</span>
    <span class="sub">{config.type === 'mix' ? `${config.mode} of ${combineSources.join(', ') || 'nothing yet'}` : config.type === 'offset' ? `${combineSources[0] || 'nothing yet'} × ${config.scale} + ${config.add}` : `follows ${combineSources[0] || 'nothing yet'}`} → {outputTargets.map((target) => target.label).join(', ') || 'unassigned'}</span>
  </div>
  {#if config.type === 'offset'}
    <div class="two">
      <label class="field">add %<input type="number" value={config.add} oninput={(e) => updateCombineField('add', numberFromEvent(e))} /></label>
      <label class="field">scale<input type="number" step="0.1" value={config.scale} oninput={(e) => updateCombineField('scale', numberFromEvent(e))} /></label>
    </div>
  {/if}
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
  {@render membership()}
  {@render nodeActions()}
{/if}

<style>
  .slider-field {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .slider-field input[type='range'] {
    flex: 1;
  }

  .slider-field .field input {
    width: 64px;
  }

  .compact-toggle {
    margin-top: 4px;
  }

  .eye {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: none;
    color: var(--muted);
    flex-shrink: 0;
  }

  .eye:hover {
    color: var(--ink);
    background: var(--surface2);
  }

  .row-hidden .row-label,
  .row-hidden .mono {
    opacity: 0.5;
  }

  .actions {
    display: flex;
    gap: 6px;
  }

  .actions .delete {
    margin-left: auto;
  }

  .row-label {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 2px 6px;
    color: inherit;
    font: inherit;
  }

  .row-label:hover,
  .row-label:focus {
    border-color: var(--line2);
    background: var(--surface2);
  }

  .limits {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    padding: 0 0 8px 12px;
  }

  .limit {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 10.5px;
    color: var(--muted);
  }

  .limit input {
    width: 100%;
    min-width: 0;
  }

  .calibrate {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 0 10px 12px;
    font-size: 11px;
  }

  .calibrate .note {
    margin: 0;
  }

  .preset-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .preset-row select {
    flex: 1;
    min-width: 0;
  }

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

  .save-warnings {
    margin: 0;
    padding-left: 1.1rem;
    color: var(--warn);
    font-size: 0.85rem;
  }

  .kind.group {
    background: var(--accent);
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
    grid-template-columns: repeat(4, 1fr);
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

  .members {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .members li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
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
