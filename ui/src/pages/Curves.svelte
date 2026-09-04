<script>
  import { onMount, onDestroy } from 'svelte';
  import { snapshot } from '../lib/store.js';
  import { getConfig, putConfig, getInventory, activateProfile } from '../lib/api.js';
  import { refreshWarnings } from '../lib/warnings.js';
  import { page } from '../lib/page.js';
  import PointCurveEditor from '../lib/components/PointCurveEditor.svelte';
  import {
    SENSOR_TYPES,
    virtualId,
    sensorOptions,
    defaultVirtualSensor,
    virtualSensorReferences,
    virtualSensorValidationError,
  } from '../lib/sensors.js';

  let config = null;
  let inventory = null;
  let editingProfile = '';
  let selectedCurveId = '';
  let newProfileName = '';
  let newCurveId = '';
  let newCurveType = 'point';
  let error = '';
  let saveWarnings = [];
  let saving = false;
  let deleteBlocked = '';
  let dirty = false;
  let selectedSensorName = '';
  let newSensorName = '';
  let newSensorType = 'max';
  let sensorDeleteBlocked = '';

  let previousPage = 'curves';
  const unsubscribePage = page.subscribe((value) => {
    if (previousPage === 'curves' && value !== 'curves' && dirty) {
      if (!confirm('You have unsaved curve changes. Leave without saving?')) {
        page.set('curves');
        return;
      }
    }
    previousPage = value;
  });
  onDestroy(unsubscribePage);

  const CURVE_TYPES = ['point', 'flat', 'mix', 'sync', 'trigger', 'target'];
  const MIX_MODES = ['max', 'min', 'avg'];

  function newPointId() {
    return crypto.randomUUID();
  }

  function tagPoints(pairs) {
    return pairs.map(([temp, duty]) => ({ id: newPointId(), temp, duty }));
  }

  function untagPoints(taggedPoints) {
    return taggedPoints.map((point) => [point.temp, point.duty]);
  }

  function tagConfigPoints(cfg) {
    for (const profileConfig of Object.values(cfg.profiles)) {
      for (const curve of Object.values(profileConfig.curves)) {
        if (curve.type === 'point') {
          curve.points = tagPoints(curve.points);
        }
      }
      profileConfig.sensors = profileConfig.sensors || {};
    }
    return cfg;
  }

  function toWireConfig(cfg) {
    const clone = JSON.parse(JSON.stringify(cfg));
    for (const profileConfig of Object.values(clone.profiles)) {
      for (const curve of Object.values(profileConfig.curves)) {
        if (curve.type === 'point') {
          curve.points = untagPoints(curve.points);
        }
      }
    }
    return clone;
  }

  onMount(load);

  async function load() {
    error = '';
    saveWarnings = [];
    try {
      const [cfg, inv] = await Promise.all([getConfig(), getInventory()]);
      config = tagConfigPoints(cfg);
      inventory = inv;
      editingProfile = cfg.active_profile;
      selectedCurveId = firstCurveId(cfg);
      dirty = false;
    } catch (err) {
      error = err.message;
    }
  }

  function firstCurveId(cfg) {
    const profile = cfg.profiles[cfg.active_profile];
    const ids = profile ? Object.keys(profile.curves) : [];
    return ids[0] || '';
  }

  function defaultCurve(type) {
    switch (type) {
      case 'point':
        return {
          type: 'point',
          sensor: '',
          points: tagPoints([
            [30, 20],
            [70, 100],
          ]),
          hysteresis: null,
          response: null,
        };
      case 'flat':
        return { type: 'flat', duty: 50 };
      case 'mix':
        return { type: 'mix', sources: [], mode: 'max' };
      case 'sync':
        return { type: 'sync', source: '' };
      case 'trigger':
        return {
          type: 'trigger',
          sensor: '',
          on_temp: 60,
          off_temp: 50,
          on_duty: 100,
          off_duty: 20,
        };
      case 'target':
        return {
          type: 'target',
          sensor: '',
          target_temp: 60,
          step_pct_per_sec: 5,
          min_duty: 20,
          max_duty: 100,
        };
      default:
        return { type: 'flat', duty: 50 };
    }
  }

  $: profileNames = config ? Object.keys(config.profiles).sort() : [];
  $: profile = config ? config.profiles[editingProfile] : null;
  $: curveIds = profile ? Object.keys(profile.curves).sort() : [];
  $: selectedCurve = profile ? profile.curves[selectedCurveId] : null;
  $: sensors = inventory ? inventory.sensors : [];
  $: controls = inventory ? inventory.controls : [];
  $: virtualNames = profile ? Object.keys(profile.sensors).sort() : [];
  $: selectedSensor = profile ? profile.sensors[selectedSensorName] : null;
  $: pickerOptions = sensorOptions(sensors, virtualNames, null);
  $: sensorInputOptions = sensorOptions(
    sensors,
    virtualNames,
    selectedSensorName ? virtualId(selectedSensorName) : null
  );

  function typeBadge(curve) {
    return curve ? curve.type : '';
  }

  async function activate(name) {
    error = '';
    try {
      await activateProfile(name);
      config.active_profile = name;
      config = config;
      await refreshWarnings();
    } catch (err) {
      error = err.message;
    }
  }

  function addProfile() {
    const name = newProfileName.trim();
    if (!name || config.profiles[name]) return;
    config.profiles[name] = { curves: {}, assignments: {}, sensors: {} };
    config = config;
    dirty = true;
    editingProfile = name;
    newProfileName = '';
  }

  function duplicateProfile(name) {
    const base = name;
    let candidate = `${base}-copy`;
    let n = 2;
    while (config.profiles[candidate]) {
      candidate = `${base}-copy${n}`;
      n += 1;
    }
    config.profiles[candidate] = JSON.parse(JSON.stringify(config.profiles[name]));
    config = config;
    dirty = true;
    editingProfile = candidate;
  }

  function deleteProfile(name) {
    if (profileNames.length <= 1) return;
    if (name === config.active_profile) return;
    delete config.profiles[name];
    config = config;
    dirty = true;
    if (editingProfile === name) {
      editingProfile = config.active_profile;
      selectedCurveId = firstCurveId(config);
    }
  }

  function referencesOf(curveId, curves, assignments) {
    const refs = [];
    for (const [id, curve] of Object.entries(curves)) {
      if (id === curveId) continue;
      if (curve.type === 'mix' && curve.sources.includes(curveId)) refs.push(`curve "${id}"`);
      if (curve.type === 'sync' && curve.source === curveId) refs.push(`curve "${id}"`);
    }
    for (const [controlId, target] of Object.entries(assignments)) {
      if (target === curveId) refs.push(`assignment "${controlId}"`);
    }
    return refs;
  }

  function addCurve() {
    const id = newCurveId.trim();
    if (!id || profile.curves[id]) return;
    profile.curves[id] = defaultCurve(newCurveType);
    config = config;
    dirty = true;
    selectedCurveId = id;
    newCurveId = '';
  }

  function deleteCurve(id) {
    const refs = referencesOf(id, profile.curves, profile.assignments);
    if (refs.length > 0) {
      deleteBlocked = `Cannot delete "${id}": referenced by ${refs.join(', ')}.`;
      return;
    }
    deleteBlocked = '';
    delete profile.curves[id];
    config = config;
    dirty = true;
    if (selectedCurveId === id) {
      selectedCurveId = firstCurveId(config);
    }
  }

  function setCurveField(field, value) {
    selectedCurve[field] = value;
    config = config;
    dirty = true;
  }

  function toggleHysteresis(enabled) {
    selectedCurve.hysteresis = enabled ? { up: 2, down: 5 } : null;
    config = config;
    dirty = true;
  }

  function toggleResponse(enabled) {
    selectedCurve.response = enabled ? { rise_pct_per_sec: 10, fall_pct_per_sec: 10 } : null;
    config = config;
    dirty = true;
  }

  function toggleMixSource(sourceId, checked) {
    if (checked) {
      if (!selectedCurve.sources.includes(sourceId)) {
        selectedCurve.sources = [...selectedCurve.sources, sourceId];
      }
    } else {
      selectedCurve.sources = selectedCurve.sources.filter((s) => s !== sourceId);
    }
    config = config;
    dirty = true;
  }

  function setAssignment(controlId, curveId) {
    if (curveId) {
      profile.assignments[controlId] = curveId;
    } else {
      delete profile.assignments[controlId];
    }
    config = config;
    dirty = true;
  }

  function addSensor() {
    const name = newSensorName.trim();
    if (!name || profile.sensors[name]) return;
    profile.sensors[name] = defaultVirtualSensor(newSensorType);
    config = config;
    dirty = true;
    selectedSensorName = name;
    newSensorName = '';
  }

  function deleteSensor(name) {
    const refs = virtualSensorReferences(name, profile.curves, profile.sensors);
    if (refs.length > 0) {
      sensorDeleteBlocked = `Cannot delete "${name}": referenced by ${refs.join(', ')}.`;
      return;
    }
    sensorDeleteBlocked = '';
    delete profile.sensors[name];
    config = config;
    dirty = true;
    if (selectedSensorName === name) {
      selectedSensorName = '';
    }
  }

  function setSensorField(field, value) {
    selectedSensor[field] = value;
    config = config;
    dirty = true;
  }

  function toggleSensorInput(id, checked) {
    if (checked) {
      if (!selectedSensor.inputs.includes(id)) {
        selectedSensor.inputs = [...selectedSensor.inputs, id];
      }
    } else {
      selectedSensor.inputs = selectedSensor.inputs.filter((i) => i !== id);
    }
    config = config;
    dirty = true;
  }

  function toggleSensorWindow(enabled) {
    selectedSensor.window_s = enabled ? 10 : null;
    config = config;
    dirty = true;
  }

  $: liveTemp = liveTempFor(selectedCurve && selectedCurve.sensor, $snapshot);

  function liveTempFor(sensorId, snap) {
    if (!sensorId || !snap || !snap.sensors) return null;
    const value = snap.sensors[sensorId];
    return value === null || value === undefined ? null : value;
  }

  $: liveValue = selectedSensorName
    ? liveTempFor(virtualId(selectedSensorName), $snapshot)
    : null;

  $: unknownAssignments = profile
    ? Object.keys(profile.assignments).filter(
        (controlId) => !controls.some((c) => c.id === controlId)
      )
    : [];

  function isBadNumber(value) {
    return value === null || value === undefined || typeof value !== 'number' || Number.isNaN(value);
  }

  function validationError(wireConfig) {
    for (const profileConfig of Object.values(wireConfig.profiles)) {
      for (const [curveId, curve] of Object.entries(profileConfig.curves)) {
        const label = `Curve "${curveId}"`;
        if (curve.type === 'point') {
          for (const [temp, duty] of curve.points) {
            if (isBadNumber(temp)) return `${label}: a point's temperature must be a number`;
            if (isBadNumber(duty)) return `${label}: a point's duty must be a number`;
          }
          if (curve.hysteresis) {
            if (isBadNumber(curve.hysteresis.up)) return `${label}: hysteresis up must be a number`;
            if (isBadNumber(curve.hysteresis.down)) return `${label}: hysteresis down must be a number`;
          }
          if (curve.response) {
            if (isBadNumber(curve.response.rise_pct_per_sec))
              return `${label}: response rise %/s must be a number`;
            if (isBadNumber(curve.response.fall_pct_per_sec))
              return `${label}: response fall %/s must be a number`;
          }
        } else if (curve.type === 'flat') {
          if (isBadNumber(curve.duty)) return `${label}: duty must be a number`;
        } else if (curve.type === 'trigger') {
          for (const field of ['on_temp', 'off_temp', 'on_duty', 'off_duty']) {
            if (isBadNumber(curve[field])) return `${label}: ${field} must be a number`;
          }
        } else if (curve.type === 'target') {
          for (const field of ['target_temp', 'step_pct_per_sec', 'min_duty', 'max_duty']) {
            if (isBadNumber(curve[field])) return `${label}: ${field} must be a number`;
          }
        }
      }
      for (const [name, sensor] of Object.entries(profileConfig.sensors)) {
        const invalid = virtualSensorValidationError(name, sensor);
        if (invalid) return invalid;
      }
    }
    return '';
  }

  async function save() {
    saving = true;
    error = '';
    saveWarnings = [];
    const wireConfig = toWireConfig(config);
    const invalid = validationError(wireConfig);
    if (invalid) {
      error = invalid;
      saving = false;
      return;
    }
    try {
      const result = await putConfig(wireConfig);
      saveWarnings = (result && result.warnings) || [];
      dirty = false;
      await refreshWarnings();
    } catch (err) {
      error = err.message;
    } finally {
      saving = false;
    }
  }

  async function revert() {
    await load();
  }
</script>

<section>
  <h2>Curves</h2>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if !config}
    <p class="muted">Loading configuration.</p>
  {:else}
    <div class="profile-bar">
      {#each profileNames as name}
        <div class="profile-tab" class:selected={name === editingProfile}>
          <button class="tab-name" on:click={() => (editingProfile = name)}>
            {name}
            {#if name === config.active_profile}<span class="active-mark">active</span>{/if}
          </button>
          {#if name !== config.active_profile}
            <button class="mini" on:click={() => activate(name)}>Activate</button>
          {/if}
          <button class="mini" on:click={() => duplicateProfile(name)}>Duplicate</button>
          <button
            class="mini"
            disabled={profileNames.length <= 1 || name === config.active_profile}
            on:click={() => deleteProfile(name)}
          >
            Delete
          </button>
        </div>
      {/each}
      <div class="new-profile">
        <input placeholder="new profile name" bind:value={newProfileName} />
        <button class="mini" on:click={addProfile}>Add profile</button>
      </div>
    </div>

    {#if profile}
      <div class="layout">
        <div class="curve-list">
          <h3>Curves</h3>
          <ul>
            {#each curveIds as id}
              <li class:selected={id === selectedCurveId}>
                <button class="curve-name" on:click={() => (selectedCurveId = id)}>
                  {id}
                  <span class="type-badge">{typeBadge(profile.curves[id])}</span>
                </button>
                <button class="mini" on:click={() => deleteCurve(id)}>Delete</button>
              </li>
            {/each}
          </ul>
          {#if deleteBlocked}
            <p class="error small">{deleteBlocked}</p>
          {/if}
          <div class="new-curve">
            <input placeholder="curve id" bind:value={newCurveId} />
            <select bind:value={newCurveType}>
              {#each CURVE_TYPES as type}
                <option value={type}>{type}</option>
              {/each}
            </select>
            <button class="mini" on:click={addCurve}>Add curve</button>
          </div>
        </div>

        <div class="curve-editor">
          {#if selectedCurve}
            <h3>{selectedCurveId} <span class="type-badge">{selectedCurve.type}</span></h3>

            {#if selectedCurve.type === 'point'}
              <label>
                Sensor
                <select
                  value={selectedCurve.sensor}
                  on:change={(e) => setCurveField('sensor', e.target.value)}
                >
                  <option value="">(none)</option>
                  {#each pickerOptions as option}
                    <option value={option.id}>{option.label}</option>
                  {/each}
                </select>
              </label>

              <PointCurveEditor
                points={selectedCurve.points}
                {liveTemp}
                onChange={(points) => setCurveField('points', points)}
              />

              <fieldset>
                <legend>
                  <label>
                    <input
                      type="checkbox"
                      checked={selectedCurve.hysteresis !== null}
                      on:change={(e) => toggleHysteresis(e.target.checked)}
                    />
                    Hysteresis
                  </label>
                </legend>
                {#if selectedCurve.hysteresis}
                  <label class="inline">
                    up
                    <input
                      type="number"
                      bind:value={selectedCurve.hysteresis.up}
                      on:input={() => (dirty = true)}
                    />
                  </label>
                  <label class="inline">
                    down
                    <input
                      type="number"
                      bind:value={selectedCurve.hysteresis.down}
                      on:input={() => (dirty = true)}
                    />
                  </label>
                {/if}
              </fieldset>

              <fieldset>
                <legend>
                  <label>
                    <input
                      type="checkbox"
                      checked={selectedCurve.response !== null}
                      on:change={(e) => toggleResponse(e.target.checked)}
                    />
                    Response limiting
                  </label>
                </legend>
                {#if selectedCurve.response}
                  <label class="inline">
                    rise %/s
                    <input
                      type="number"
                      bind:value={selectedCurve.response.rise_pct_per_sec}
                      on:input={() => (dirty = true)}
                    />
                  </label>
                  <label class="inline">
                    fall %/s
                    <input
                      type="number"
                      bind:value={selectedCurve.response.fall_pct_per_sec}
                      on:input={() => (dirty = true)}
                    />
                  </label>
                {/if}
              </fieldset>
            {:else if selectedCurve.type === 'flat'}
              <label class="inline">
                Duty %
                <input
                  type="number"
                  min="0"
                  max="100"
                  bind:value={selectedCurve.duty}
                  on:input={() => (dirty = true)}
                />
              </label>
            {:else if selectedCurve.type === 'mix'}
              <label>
                Mode
                <select bind:value={selectedCurve.mode} on:change={() => (dirty = true)}>
                  {#each MIX_MODES as mode}
                    <option value={mode}>{mode}</option>
                  {/each}
                </select>
              </label>
              <p>Sources</p>
              <div class="checkbox-list">
                {#each curveIds.filter((id) => id !== selectedCurveId) as id}
                  <label class="inline">
                    <input
                      type="checkbox"
                      checked={selectedCurve.sources.includes(id)}
                      on:change={(e) => toggleMixSource(id, e.target.checked)}
                    />
                    {id}
                  </label>
                {/each}
              </div>
            {:else if selectedCurve.type === 'sync'}
              <label>
                Source curve
                <select
                  value={selectedCurve.source}
                  on:change={(e) => setCurveField('source', e.target.value)}
                >
                  <option value="">(none)</option>
                  {#each curveIds.filter((id) => id !== selectedCurveId) as id}
                    <option value={id}>{id}</option>
                  {/each}
                </select>
              </label>
            {:else if selectedCurve.type === 'trigger'}
              <label>
                Sensor
                <select
                  value={selectedCurve.sensor}
                  on:change={(e) => setCurveField('sensor', e.target.value)}
                >
                  <option value="">(none)</option>
                  {#each pickerOptions as option}
                    <option value={option.id}>{option.label}</option>
                  {/each}
                </select>
              </label>
              <label class="inline">
                On temp
                <input
                  type="number"
                  bind:value={selectedCurve.on_temp}
                  on:input={() => (dirty = true)}
                />
              </label>
              <label class="inline">
                Off temp
                <input
                  type="number"
                  bind:value={selectedCurve.off_temp}
                  on:input={() => (dirty = true)}
                />
              </label>
              <label class="inline">
                On duty %
                <input
                  type="number"
                  bind:value={selectedCurve.on_duty}
                  on:input={() => (dirty = true)}
                />
              </label>
              <label class="inline">
                Off duty %
                <input
                  type="number"
                  bind:value={selectedCurve.off_duty}
                  on:input={() => (dirty = true)}
                />
              </label>
            {:else if selectedCurve.type === 'target'}
              <label>
                Sensor
                <select
                  value={selectedCurve.sensor}
                  on:change={(e) => setCurveField('sensor', e.target.value)}
                >
                  <option value="">(none)</option>
                  {#each pickerOptions as option}
                    <option value={option.id}>{option.label}</option>
                  {/each}
                </select>
              </label>
              <label class="inline">
                Target temp
                <input
                  type="number"
                  bind:value={selectedCurve.target_temp}
                  on:input={() => (dirty = true)}
                />
              </label>
              <label class="inline">
                Step %/s
                <input
                  type="number"
                  bind:value={selectedCurve.step_pct_per_sec}
                  on:input={() => (dirty = true)}
                />
              </label>
              <label class="inline">
                Min duty %
                <input
                  type="number"
                  bind:value={selectedCurve.min_duty}
                  on:input={() => (dirty = true)}
                />
              </label>
              <label class="inline">
                Max duty %
                <input
                  type="number"
                  bind:value={selectedCurve.max_duty}
                  on:input={() => (dirty = true)}
                />
              </label>
            {/if}
          {:else}
            <p class="muted">No curve selected.</p>
          {/if}
        </div>
      </div>

      <section class="virtual-sensors">
        <h3>Virtual sensors</h3>
        <ul class="sensor-list">
          {#each virtualNames as name}
            <li class:selected={name === selectedSensorName}>
              <button class="curve-name" on:click={() => (selectedSensorName = name)}>
                {name}
                <span class="type-badge">{profile.sensors[name].type}</span>
              </button>
              <button class="mini" on:click={() => deleteSensor(name)}>Delete</button>
            </li>
          {/each}
        </ul>
        {#if sensorDeleteBlocked}
          <p class="error small">{sensorDeleteBlocked}</p>
        {/if}
        <div class="new-sensor">
          <input placeholder="sensor name" bind:value={newSensorName} />
          <select bind:value={newSensorType}>
            {#each SENSOR_TYPES as type}
              <option value={type}>{type}</option>
            {/each}
          </select>
          <button class="mini" on:click={addSensor}>Add sensor</button>
        </div>

        {#if selectedSensor}
          <h4>
            {selectedSensorName} <span class="type-badge">{selectedSensor.type}</span>
          </h4>

          {#if selectedSensor.type === 'max' || selectedSensor.type === 'min' || selectedSensor.type === 'mean'}
            <p>Inputs</p>
            <div class="checkbox-list">
              {#each sensorInputOptions as option}
                <label class="inline">
                  <input
                    type="checkbox"
                    checked={selectedSensor.inputs.includes(option.id)}
                    on:change={(e) => toggleSensorInput(option.id, e.target.checked)}
                  />
                  {option.label}
                </label>
              {/each}
            </div>

            {#if selectedSensor.type === 'mean'}
              <fieldset>
                <legend>
                  <label>
                    <input
                      type="checkbox"
                      checked={selectedSensor.window_s !== null}
                      on:change={(e) => toggleSensorWindow(e.target.checked)}
                    />
                    Moving average window
                  </label>
                </legend>
                {#if selectedSensor.window_s !== null}
                  <label class="inline">
                    window_s
                    <input
                      type="number"
                      bind:value={selectedSensor.window_s}
                      on:input={() => (dirty = true)}
                    />
                  </label>
                {/if}
              </fieldset>
            {/if}
          {:else if selectedSensor.type === 'offset'}
            <label>
              Input
              <select
                value={selectedSensor.input}
                on:change={(e) => setSensorField('input', e.target.value)}
              >
                <option value="">(none)</option>
                {#each sensorInputOptions as option}
                  <option value={option.id}>{option.label}</option>
                {/each}
              </select>
            </label>
            <label class="inline">
              add
              <input
                type="number"
                bind:value={selectedSensor.add}
                on:input={() => (dirty = true)}
              />
            </label>
            <label class="inline">
              scale
              <input
                type="number"
                bind:value={selectedSensor.scale}
                on:input={() => (dirty = true)}
              />
            </label>
          {:else if selectedSensor.type === 'delta'}
            <label>
              Input
              <select
                value={selectedSensor.input}
                on:change={(e) => setSensorField('input', e.target.value)}
              >
                <option value="">(none)</option>
                {#each sensorInputOptions as option}
                  <option value={option.id}>{option.label}</option>
                {/each}
              </select>
            </label>
            <label class="inline">
              window_s
              <input
                type="number"
                bind:value={selectedSensor.window_s}
                on:input={() => (dirty = true)}
              />
            </label>
          {/if}

          <p class="mini">
            Live: {liveValue === null || liveValue === undefined ? 'n/a' : liveValue}
          </p>
        {/if}
      </section>

      <h3>Assignments</h3>
      <table class="assignments">
        <thead>
          <tr>
            <th>Control</th>
            <th>Curve</th>
          </tr>
        </thead>
        <tbody>
          {#each controls as control}
            <tr>
              <td>{control.label}</td>
              <td>
                <select
                  value={profile.assignments[control.id] || ''}
                  on:change={(e) => setAssignment(control.id, e.target.value)}
                >
                  <option value="">None</option>
                  {#each curveIds as id}
                    <option value={id}>{id}</option>
                  {/each}
                </select>
              </td>
            </tr>
          {/each}
          {#each unknownAssignments as controlId}
            <tr class="unknown">
              <td>
                <span class="warning-icon" title="unknown hardware">&#9888;</span>
                {controlId}
              </td>
              <td>
                <select
                  value={profile.assignments[controlId]}
                  on:change={(e) => setAssignment(controlId, e.target.value)}
                >
                  <option value="">None</option>
                  {#each curveIds as id}
                    <option value={id}>{id}</option>
                  {/each}
                </select>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    <div class="actions">
      <button on:click={save} disabled={saving}>Save</button>
      <button class="secondary" on:click={revert} disabled={saving}>Revert</button>
    </div>

    {#if saveWarnings.length > 0}
      <ul class="save-warnings">
        {#each saveWarnings as warning}
          <li>{warning}</li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .error {
    color: #f87171;
  }

  .error.small {
    font-size: 0.85rem;
  }

  .muted {
    opacity: 0.6;
  }

  .profile-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 1rem;
    padding-bottom: 0.75rem;
    border-bottom: 1px solid #2a2f38;
  }

  .profile-tab {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    background: #1b1e24;
    border: 1px solid #2a2f38;
    border-radius: 6px;
    padding: 0.2rem 0.4rem;
  }

  .profile-tab.selected {
    border-color: #38bdf8;
  }

  .tab-name {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    font-weight: 600;
  }

  .active-mark {
    margin-left: 0.4rem;
    font-size: 0.7rem;
    color: #4ade80;
    font-weight: normal;
  }

  .new-profile {
    display: flex;
    gap: 0.4rem;
    margin-left: auto;
  }

  .layout {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 1.5rem;
  }

  .curve-list ul,
  .sensor-list {
    list-style: none;
    padding: 0;
    margin: 0.5rem 0;
  }

  .curve-list li,
  .sensor-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.3rem 0.4rem;
    border-radius: 4px;
  }

  .curve-list li.selected,
  .sensor-list li.selected {
    background: #1b1e24;
  }

  .curve-name {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    text-align: left;
    flex: 1;
  }

  .type-badge {
    background: #2a2f38;
    border-radius: 999px;
    font-size: 0.7rem;
    padding: 0.05rem 0.45rem;
    margin-left: 0.4rem;
    opacity: 0.85;
  }

  .new-curve {
    display: flex;
    gap: 0.4rem;
    margin-top: 0.5rem;
  }

  .new-curve input {
    width: 6rem;
  }

  label {
    display: block;
    margin-bottom: 0.5rem;
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
    border: 1px solid #2a2f38;
    border-radius: 4px;
    padding: 0.25rem 0.4rem;
  }

  input[type='number'] {
    width: 5rem;
  }

  fieldset {
    border: 1px solid #2a2f38;
    border-radius: 6px;
    margin: 0.75rem 0;
  }

  .checkbox-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  table.assignments {
    width: 100%;
    border-collapse: collapse;
    margin-top: 0.5rem;
  }

  table.assignments th,
  table.assignments td {
    text-align: left;
    padding: 0.35rem 0.5rem;
    border-bottom: 1px solid #2a2f38;
  }

  tr.unknown {
    color: #fbbf24;
  }

  .warning-icon {
    margin-right: 0.3rem;
  }

  .actions {
    display: flex;
    gap: 0.6rem;
    margin-top: 1.25rem;
  }

  button {
    background: #2a2f38;
    color: inherit;
    border: none;
    border-radius: 4px;
    padding: 0.35rem 0.8rem;
    cursor: pointer;
  }

  button.secondary {
    background: none;
    border: 1px solid #2a2f38;
  }

  button.mini {
    padding: 0.15rem 0.5rem;
    font-size: 0.8rem;
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .save-warnings {
    margin-top: 0.75rem;
    color: #fbbf24;
  }

  .virtual-sensors {
    margin-top: 1.5rem;
  }

  .new-sensor {
    display: flex;
    gap: 0.4rem;
    margin-top: 0.5rem;
  }
</style>
