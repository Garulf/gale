<script>
  import PointCurveEditor from '../../components/PointCurveEditor.svelte';
  import { snapshot } from '../../store.js';
  import { tempValue, dutyValue, formatTemp, formatDuty } from '../liveValues.js';

  let { node, edges, onUpdateData, onDeleteNode } = $props();

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

  let selectedId = $derived(node ? node.id : null);
  let taggedPoints = $state([]);
  let lastTaggedFor = null;

  $effect(() => {
    const sid = selectedId;
    if (sid !== lastTaggedFor) {
      lastTaggedFor = sid;
      if (node && node.type === 'curve' && node.data.curve.config.type === 'point') {
        taggedPoints = tagPoints(node.data.curve.config.points);
      } else {
        taggedPoints = [];
      }
    }
  });

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
    onUpdateData(node.id, (data) => ({
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
    onUpdateData(node.id, (data) => ({
      ...data,
      combine: { ...data.combine, config: { ...data.combine.config, [field]: value } },
    }));
  }

  function updateVirtualField(field, value) {
    onUpdateData(node.id, (data) => ({
      ...data,
      virtual: { ...data.virtual, config: { ...data.virtual.config, [field]: value } },
    }));
  }

  function toggleVirtualWindow(enabled) {
    updateVirtualField('window_s', enabled ? 10 : null);
  }

  function numberFromEvent(event) {
    return Number(event.target.value);
  }
</script>

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
  {/if}

  <p class="mini">Live: {liveVirtualValue === null ? 'n/a' : formatTemp(liveVirtualValue)}</p>
  <div class="btns">
    <button type="button" onclick={() => onDeleteNode(node.id)}>Delete node</button>
  </div>
{:else if node.type === 'curve'}
  {@const config = node.data.curve.config}
  <h3>{node.data.curve.id} <small>{config.type}</small></h3>

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
</style>
