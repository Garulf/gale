<script>
  import { onMount, onDestroy, setContext, tick } from 'svelte';
  import { SvelteFlow, Background } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import '../lib/graph/styles.css';
  import { getConfig, putConfig, getInventory, putLabel, deleteLabel } from '../lib/api.js';
  import { snapshot } from '../lib/store.js';
  import { warnings, refreshWarnings } from '../lib/warnings.js';
  import { refreshConfig } from '../lib/config.js';
  import { page, graphFocus } from '../lib/page.js';
  import { configToGraph, graphToConfig, edgeInto, replaceEdge, withWiredRows } from '../lib/graph/model.js';
  import { isValidConnection } from '../lib/graph/validate.js';
  import { autoLayout } from '../lib/graph/layout.js';
  import { nodeKind, edgeId } from '../lib/graph/ids.js';
  import { defaultVirtualSensor } from '../lib/sensors.js';
  import { defaultCurve } from '../lib/graph/defaults.js';
  import { configValidationError } from '../lib/graph/configValidation.js';
  import { collectNodeWarnings, attributeWarning } from '../lib/graph/nodeWarnings.js';
  import { renameNode, changeVirtualType, changeCurveType, nameInUse, applyPreset, duplicateNode } from '../lib/graph/edit.js';
  import { refreshPresets } from '../lib/presets.js';
  import { shortDevice } from '../lib/dashboard.js';
  import DeviceSensorNode from '../lib/graph/components/DeviceSensorNode.svelte';
  import DeviceControlNode from '../lib/graph/components/DeviceControlNode.svelte';
  import VirtualNode from '../lib/graph/components/VirtualNode.svelte';
  import CurveNode from '../lib/graph/components/CurveNode.svelte';
  import CombineNode from '../lib/graph/components/CombineNode.svelte';
  import GaleEdge from '../lib/graph/components/GaleEdge.svelte';
  import NodePanel from '../lib/graph/components/NodePanel.svelte';
  import GraphToolbar from '../lib/graph/components/GraphToolbar.svelte';
  import ZoomControls from '../lib/graph/components/ZoomControls.svelte';
  import ChainView from '../lib/graph/components/ChainView.svelte';

  let config = $state.raw(null);
  let inventory = $state.raw(null);
  let editingProfile = $state('');
  let nodes = $state.raw([]);
  let edges = $state.raw([]);
  let savedEdgeIds = $state.raw(new Set());
  let selectedNodeId = $state('');
  let dirty = $state(false);
  let history = $state([]);
  let future = $state([]);
  let showEdgeLabels = $state(true);
  let error = $state('');
  let saveWarnings = $state([]);
  let saving = $state(false);
  let savedAt = $state(0);
  let mobileView = $state('chains');
  const PANEL_WIDTH_KEY = 'gale.graph.panelWidth';
  const PANEL_MIN = 260;
  const PANEL_MAX = 720;
  let panelWidth = $state(readPanelWidth());
  let resizing = $state(false);

  function readPanelWidth() {
    try {
      const stored = Number(localStorage.getItem(PANEL_WIDTH_KEY));
      if (Number.isFinite(stored) && stored >= PANEL_MIN && stored <= PANEL_MAX) return stored;
    } catch (err) {
      return 330;
    }
    return 330;
  }

  function clampPanelWidth(width) {
    const viewportMax = Math.min(PANEL_MAX, Math.max(PANEL_MIN, window.innerWidth - 480));
    return Math.min(viewportMax, Math.max(PANEL_MIN, Math.round(width)));
  }

  function startPanelResize(event) {
    event.preventDefault();
    resizing = true;
    const startX = event.clientX;
    const startWidth = panelWidth;
    const move = (moveEvent) => {
      panelWidth = clampPanelWidth(startWidth + (startX - moveEvent.clientX));
    };
    const stop = () => {
      resizing = false;
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', stop);
      try {
        localStorage.setItem(PANEL_WIDTH_KEY, String(panelWidth));
      } catch (err) {
        return;
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', stop);
  }

  function onViewportResize() {
    panelWidth = clampPanelWidth(panelWidth);
  }
  let sheetOpen = $state(false);
  const mobileQuery = window.matchMedia('(max-width: 720px)');
  let isMobile = $state(mobileQuery.matches);
  const onMobileChange = (event) => (isMobile = event.matches);
  mobileQuery.addEventListener('change', onMobileChange);
  onDestroy(() => mobileQuery.removeEventListener('change', onMobileChange));
  const nodeTypes = {
    deviceSensor: DeviceSensorNode,
    deviceControl: DeviceControlNode,
    virtual: VirtualNode,
    curve: CurveNode,
    combine: CombineNode,
  };
  const edgeTypes = { gale: GaleEdge };

  function hideNode(id) {
    snapshotHistory();
    nodes = nodes.map((node) => (node.id === id ? { ...node, hidden: true } : node));
    if (selectedNodeId === id) selectedNodeId = '';
    dirty = true;
    future = [];
  }

  function showNode(id) {
    snapshotHistory();
    nodes = nodes.map((node) => (node.id === id ? { ...node, hidden: false } : node));
    dirty = true;
    future = [];
  }

  setContext('galeHideNode', hideNode);
  setContext('galeUpdateNodeData', (id, updater, shouldSnapshot) => updateNodeData(id, updater, shouldSnapshot));

  function toggleCompact(id) {
    snapshotHistory();
    nodes = nodes.map((node) => (node.id === id ? { ...node, compact: node.compact !== true } : node));
    dirty = true;
    future = [];
  }

  let nodeWarnings = $derived(collectNodeWarnings(nodes, edges, [...$warnings, ...saveWarnings]));
  setContext('galeNodeWarnings', () => nodeWarnings);
  setContext('galeEdgeSaved', (edgeId) => savedEdgeIds.has(edgeId));
  setContext('galeNodeCompact', (nodeId) => nodes.some((node) => node.id === nodeId && node.compact === true));

  let previousPage = 'graph';
  const unsubscribePage = page.subscribe((value) => {
    if (previousPage === 'graph' && value !== 'graph' && dirty) {
      if (!confirm('You have unsaved changes on this profile. Leave without saving?')) {
        page.set('graph');
        return;
      }
    }
    previousPage = value;
  });
  onDestroy(unsubscribePage);

  onMount(load);

  function toFlowEdges(rawEdges, labelsOn) {
    return rawEdges.map((edge) => ({
      ...edge,
      type: 'gale',
      class: edge.data.kind,
      data: { ...edge.data, showLabel: labelsOn },
    }));
  }

  async function load() {
    error = '';
    saveWarnings = [];
    refreshPresets();
    try {
      const [cfg, inv] = await Promise.all([getConfig(), getInventory()]);
      config = cfg;
      inventory = inv;
      editingProfile = cfg.active_profile;
      const graph = configToGraph(config, inventory, editingProfile);
      nodes = graph.nodes;
      edges = toFlowEdges(graph.edges, showEdgeLabels);
      savedEdgeIds = new Set(edges.map((edge) => edge.id));
      selectedNodeId = '';
      history = [];
      future = [];
      dirty = false;
      applyFocus();
    } catch (err) {
      error = err.message;
    }
  }

  let activeProfile = $derived($snapshot ? $snapshot.active_profile : config ? config.active_profile : '');

  function rebuildGraph(profile) {
    editingProfile = profile;
    const graph = configToGraph(config, inventory, profile);
    nodes = graph.nodes;
    edges = toFlowEdges(graph.edges, showEdgeLabels);
    savedEdgeIds = new Set(edges.map((edge) => edge.id));
    selectedNodeId = '';
    history = [];
    future = [];
    dirty = false;
  }

  let profileChangedTo = $state('');

  $effect(() => {
    const rewired = withWiredRows(nodes, edges);
    if (rewired !== nodes) nodes = rewired;
  });

  $effect(() => {
    const active = activeProfile;
    if (!config || !active || active === editingProfile) return;
    if (dirty) {
      profileChangedTo = active;
    } else {
      profileChangedTo = '';
      reloadForActiveProfile();
    }
  });

  async function reloadForActiveProfile() {
    try {
      config = await getConfig();
      rebuildGraph(config.active_profile);
      profileChangedTo = '';
    } catch (err) {
      error = err.message;
    }
  }

  function applyFocus() {
    const focus = $graphFocus;
    if (!focus) return;
    graphFocus.set(null);
    let target = focus.nodeId || '';
    if (!target && focus.warning) {
      target = attributeWarning(focus.warning, nodes, edges)[0] || '';
    }
    if (target && nodes.some((node) => node.id === target)) {
      selectNode(target);
    }
  }

  function selectNode(id) {
    selectedNodeId = id;
    sheetOpen = true;
    nodes = nodes.map((node) => (node.selected !== (node.id === id) ? { ...node, selected: node.id === id } : node));
  }

  function cloneData(value) {
    return JSON.parse(JSON.stringify(value));
  }

  function cloneGraph(rawNodes, rawEdges) {
    return {
      nodes: rawNodes.map((node) => ({
        id: node.id,
        type: node.type,
        position: { x: node.position.x, y: node.position.y },
        hidden: node.hidden === true,
        compact: node.compact === true,
        data: cloneData(node.data),
      })),
      edges: rawEdges.map((edge) => ({
        id: edge.id,
        source: edge.source,
        sourceHandle: edge.sourceHandle,
        target: edge.target,
        targetHandle: edge.targetHandle,
        type: edge.type,
        class: edge.class,
        data: cloneData(edge.data),
      })),
    };
  }

  function snapshotHistory() {
    history = [...history, cloneGraph(nodes, edges)];
  }

  function pruneStaleSelection() {
    if (selectedNodeId && !nodes.some((node) => node.id === selectedNodeId)) {
      selectedNodeId = '';
    }
  }

  function undo() {
    if (history.length === 0) return;
    future = [cloneGraph(nodes, edges), ...future];
    const previous = history[history.length - 1];
    history = history.slice(0, -1);
    nodes = previous.nodes;
    edges = previous.edges;
    dirty = true;
    pruneStaleSelection();
  }

  function redo() {
    if (future.length === 0) return;
    history = [...history, cloneGraph(nodes, edges)];
    const next = future[0];
    future = future.slice(1);
    nodes = next.nodes;
    edges = next.edges;
    dirty = true;
    pruneStaleSelection();
  }

  function onNodeDragStart() {
    snapshotHistory();
  }

  function onNodeDragStop() {
    dirty = true;
    future = [];
  }

  function buildEdge(connection) {
    const sourceKind = nodeKind(connection.source);
    const kind = sourceKind === 'sensor' || sourceKind === 'virtual' ? 'temp' : 'duty';
    return {
      id: edgeId(connection.source, connection.sourceHandle, connection.target, connection.targetHandle),
      source: connection.source,
      sourceHandle: connection.sourceHandle,
      target: connection.target,
      targetHandle: connection.targetHandle,
      type: 'gale',
      class: kind,
      data: { kind, showLabel: showEdgeLabels },
    };
  }

  function targetLabel(nodeId, handle) {
    const node = nodes.find((candidate) => candidate.id === nodeId);
    if (!node) return `${nodeId}: ${handle}`;
    if (node.type === 'deviceControl') {
      const row = node.data.deviceControl.rows.find((candidate) => candidate.handle === handle);
      return `${node.data.deviceControl.device}: ${row ? row.label : handle}`;
    }
    const name = node.type === 'virtual' ? node.data.virtual.name : node.data[node.type].id;
    return `${name}: ${handle}`;
  }

  function commitEdges(nextEdges) {
    snapshotHistory();
    edges = nextEdges;
    dirty = true;
    future = [];
  }

  function onBeforeConnect(connection) {
    if (!isValidConnection(connection, nodes, edges)) return false;
    const newEdge = buildEdge(connection);
    const existing = edgeInto(edges, connection.target, connection.targetHandle);
    if (existing && existing.id === newEdge.id) return false;
    if (existing) {
      const label = targetLabel(connection.target, connection.targetHandle);
      if (!window.confirm(`Replace the existing connection into "${label}"?`)) return false;
      commitEdges(replaceEdge(edges, newEdge, connection.target, connection.targetHandle));
      return false;
    }
    commitEdges([...edges, newEdge]);
    return false;
  }

  function onBeforeDelete({ nodes: deletedNodes }) {
    const blocksDelete = deletedNodes.some(
      (node) => node.type === 'deviceSensor' || node.type === 'deviceControl'
    );
    if (blocksDelete) return false;
    snapshotHistory();
    dirty = true;
    future = [];
    return true;
  }

  function onNodeClick({ node }) {
    selectedNodeId = node.id;
    sheetOpen = true;
  }

  function onPaneClick() {
    selectedNodeId = '';
  }

  function updateNodeData(id, updater, shouldSnapshot = true) {
    if (shouldSnapshot) snapshotHistory();
    nodes = nodes.map((node) => (node.id === id ? { ...node, data: updater(node.data) } : node));
    dirty = true;
    future = [];
  }

  function applyEdit(edit) {
    snapshotHistory();
    nodes = edit.nodes;
    edges = edit.edges;
    if (selectedNodeId !== edit.id) selectedNodeId = edit.id;
    dirty = true;
    future = [];
  }

  function renameGraphNode(id, name) {
    if (nameInUse(nodes, nodeKind(id), name, id)) return `Name "${name}" is already in use`;
    applyEdit(renameNode(nodes, edges, id, name));
    return '';
  }

  function retypeNode(id, type) {
    const edit = nodeKind(id) === 'virtual' ? changeVirtualType(nodes, edges, id, type) : changeCurveType(nodes, edges, id, type);
    if (edit.nodes === nodes) return;
    applyEdit(edit);
  }

  const DUPLICATE_OFFSET = 40;
  let copiedNodeId = '';

  function duplicateGraphNode(id) {
    const source = nodes.find((node) => node.id === id);
    if (!source) return;
    const result = duplicateNode(nodes, id, cascadePosition({ x: source.position.x + DUPLICATE_OFFSET, y: source.position.y + DUPLICATE_OFFSET }));
    if (!result) return;
    snapshotHistory();
    nodes = result.nodes;
    selectedNodeId = result.id;
    sheetOpen = true;
    dirty = true;
    future = [];
  }

  function isTypingTarget(target) {
    return target instanceof HTMLElement && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName));
  }

  function onClipboardKey(event) {
    if (!(event.ctrlKey || event.metaKey) || isTypingTarget(event.target)) return;
    const key = event.key.toLowerCase();
    if (key === 'c' && selectedNodeId && nodeKind(selectedNodeId) !== 'sensor' && nodeKind(selectedNodeId) !== 'control') {
      copiedNodeId = selectedNodeId;
      event.preventDefault();
    } else if (key === 'v' && copiedNodeId) {
      duplicateGraphNode(copiedNodeId);
      event.preventDefault();
    }
  }

  function labelsByHandle(inv) {
    const labels = new Map();
    for (const sensor of inv.sensors || []) labels.set(sensor.id, sensor.label);
    for (const control of inv.controls || []) labels.set(control.id, control.label);
    return labels;
  }

  function withInventoryLabels(currentNodes, inv) {
    const labels = labelsByHandle(inv);
    return currentNodes.map((node) => {
      if (node.type !== 'deviceSensor' && node.type !== 'deviceControl') return node;
      const key = node.type;
      const rows = node.data[key].rows.map((row) => (labels.has(row.handle) ? { ...row, label: labels.get(row.handle) } : row));
      return { ...node, data: { [key]: { ...node.data[key], rows } } };
    });
  }

  async function renameDeviceRow(nodeId, handle, label) {
    if (label) {
      await putLabel(handle, label);
    } else {
      await deleteLabel(handle).catch((err) => {
        if (!err.message.startsWith('404')) throw err;
      });
    }
    inventory = await getInventory();
    nodes = withInventoryLabels(nodes, inventory);
    history = history.map((entry) => ({ ...entry, nodes: withInventoryLabels(entry.nodes, inventory) }));
    future = future.map((entry) => ({ ...entry, nodes: withInventoryLabels(entry.nodes, inventory) }));
  }

  function toggleRowHidden(nodeId, handle) {
    snapshotHistory();
    nodes = nodes.map((node) => {
      if (node.id !== nodeId) return node;
      const key = node.type;
      const rows = node.data[key].rows.map((row) => (row.handle === handle ? { ...row, hidden: !row.hidden } : row));
      return { ...node, data: { [key]: { ...node.data[key], rows } } };
    });
    dirty = true;
    future = [];
  }

  function applyCurvePreset(id, preset) {
    applyEdit(applyPreset(nodes, edges, id, preset));
  }

  function deleteNode(id) {
    snapshotHistory();
    edges = edges.filter((edge) => edge.source !== id && edge.target !== id);
    nodes = nodes.filter((node) => node.id !== id);
    if (selectedNodeId === id) selectedNodeId = '';
    sheetOpen = false;
    dirty = true;
    future = [];
  }

  function runAutoLayout() {
    snapshotHistory();
    nodes = autoLayout(nodes, edges);
    dirty = true;
    future = [];
  }

  function toggleEdgeLabels() {
    showEdgeLabels = !showEdgeLabels;
    edges = edges.map((edge) => ({ ...edge, data: { ...edge.data, showLabel: showEdgeLabels } }));
  }

  function uniqueCurveId(prefix) {
    let n = 1;
    while (nodes.some((node) => node.id === `curve:${prefix}_${n}` || node.id === `combine:${prefix}_${n}`)) {
      n += 1;
    }
    return `${prefix}_${n}`;
  }

  function uniqueVirtualName() {
    let n = 1;
    while (nodes.some((node) => node.id === `virtual:sensor_${n}`)) {
      n += 1;
    }
    return `sensor_${n}`;
  }

  let nodeAddCount = 0;
  const NODE_ADD_OFFSET = 24;
  const NODE_ADD_OFFSET_CYCLE = 10;

  function cascadePosition(position) {
    const step = (nodeAddCount % NODE_ADD_OFFSET_CYCLE) * NODE_ADD_OFFSET;
    nodeAddCount += 1;
    return { x: position.x + step, y: position.y + step };
  }

  function appendNode(node) {
    snapshotHistory();
    nodes = [...nodes, node];
    selectedNodeId = node.id;
    sheetOpen = true;
    dirty = true;
    future = [];
  }

  function fallbackPosition() {
    return { x: 120, y: 120 };
  }

  function addPaletteNode(entry, position = fallbackPosition()) {
    if (entry.kind === 'virtual') {
      const name = uniqueVirtualName();
      appendNode({
        id: `virtual:${name}`,
        type: 'virtual',
        position: cascadePosition(position),
        hidden: false,
        data: { virtual: { name, config: defaultVirtualSensor(entry.type) } },
      });
      return;
    }
    const id = uniqueCurveId(entry.kind);
    const config = entry.mode ? { ...defaultCurve(entry.type), mode: entry.mode } : defaultCurve(entry.type);
    appendNode({
      id: `${entry.kind}:${id}`,
      type: entry.kind,
      position: cascadePosition(position),
      hidden: false,
      data: { [entry.kind]: { id, config } },
    });
  }

  function addCurveNode(position = fallbackPosition()) {
    addPaletteNode({ kind: 'curve', type: 'point' }, position);
  }


  async function save() {
    saving = true;
    error = '';
    saveWarnings = [];
    try {
      config = await getConfig();
      const wireConfig = graphToConfig(nodes, edges, config, editingProfile);
      const invalid = configValidationError(wireConfig);
      if (invalid) {
        error = invalid;
        return;
      }
      const result = await putConfig(wireConfig);
      saveWarnings = (result && result.warnings) || [];
      const saved = await getConfig();
      config = saved;
      savedEdgeIds = new Set(edges.map((edge) => edge.id));
      dirty = false;
      savedAt = Date.now();
      await Promise.all([refreshWarnings(), refreshConfig()]);
    } catch (err) {
      error = err.message;
    } finally {
      saving = false;
    }
  }

  async function discard() {
    await load();
  }

  let selectedNode = $derived(nodes.find((node) => node.id === selectedNodeId) || null);
  let hiddenNodes = $derived(nodes.filter((node) => node.hidden === true));
  let visibleNodes = $derived(nodes.filter((node) => node.hidden !== true));

  $effect(() => {
    const focus = $graphFocus;
    if (focus && nodes.length > 0) {
      tick().then(applyFocus);
    }
  });
</script>

<svelte:window onkeydown={onClipboardKey} onresize={onViewportResize} />

<section class="gale-graph-page" data-mobile-view={mobileView} style="--gale-panel-width: {panelWidth}px">
  <div class="gale-canvas" class:mobile-hidden={mobileView === 'chains'}>
    <SvelteFlow
      bind:nodes
      bind:edges
      {nodeTypes}
      {edgeTypes}
      isValidConnection={(connection) => isValidConnection(connection, nodes, edges)}
      onbeforeconnect={onBeforeConnect}
      onnodedragstart={onNodeDragStart}
      onnodedragstop={onNodeDragStop}
      onbeforedelete={onBeforeDelete}
      onnodeclick={onNodeClick}
      onpaneclick={onPaneClick}
      proOptions={{ hideAttribution: true }}
      fitView
    >
      <Background gap={20} size={1} />
      <GraphToolbar
        {showEdgeLabels}
        canUndo={history.length > 0}
        canRedo={future.length > 0}
        {saving}
        {dirty}
        onAutoLayout={runAutoLayout}
        onUndo={undo}
        onRedo={redo}
        onSave={save}
        onDiscard={discard}
        onToggleLabels={toggleEdgeLabels}
        onAddNode={addPaletteNode}
      />
      <ZoomControls />
    </SvelteFlow>

    <div class="gale-legend">
      <span><span class="swatch temp"></span>temperature</span>
      <span><span class="swatch duty"></span>duty</span>
      <span class="tip">drag headers · drag canvas to pan</span>
    </div>

    {#if hiddenNodes.length > 0}
      <div class="gale-hidden-strip">
        {#each hiddenNodes as node (node.id)}
          <div class="gale-hidden-chip">
            <span>{shortDevice(node.type === 'deviceSensor' ? node.data.deviceSensor.device : node.data.deviceControl.device)}</span>
            <button type="button" onclick={() => showNode(node.id)}>Show</button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="gale-chains" class:mobile-hidden={mobileView !== 'chains'}>
    <div class="chains-head">
      <div class="page-title">
        <span class="eyebrow">Profile · {editingProfile}</span>
        <h1>Graph</h1>
      </div>
      <div class="chains-actions">
        <button type="button" class="btn" onclick={() => (mobileView = 'canvas')}>Canvas</button>
        <button type="button" class="btn primary" onclick={() => addCurveNode()}>+ Node</button>
      </div>
    </div>
    {#if error}
      <p class="error">{error}</p>
    {/if}
    {#if isMobile}
      <ChainView nodes={visibleNodes} {edges} {selectedNodeId} onSelect={selectNode} />
    {/if}
    <div class="chains-foot">
      <button type="button" class="btn" disabled={!dirty || saving} onclick={discard}>Discard</button>
      <button type="button" class="btn" class:primary={dirty} disabled={saving} onclick={save}>Save profile</button>
    </div>
  </div>

  <aside class="gale-panel" class:resizing>
    <div class="resize-handle" class:active={resizing} role="separator" aria-orientation="vertical" aria-label="Resize panel" data-testid="panel-resize" onpointerdown={startPanelResize}></div>
    {#if profileChangedTo}
      <p class="notice">Active profile is now "{profileChangedTo}". You have unsaved edits here. <button type="button" class="btn" data-testid="graph-reload-profile" onclick={reloadForActiveProfile}>Discard and switch</button></p>
    {/if}
    {#if error}
      <p class="error">{error}</p>
    {/if}
    <NodePanel
      node={selectedNode}
      {nodes}
      {edges}
      {savedAt}
      onUpdateData={updateNodeData}
      onDeleteNode={deleteNode}
      onRenameNode={renameGraphNode}
      onRetypeNode={retypeNode}
      onApplyPreset={applyCurvePreset}
      onDuplicateNode={duplicateGraphNode}
      onRenameRow={renameDeviceRow}
      onToggleRow={toggleRowHidden}
      onToggleCompact={toggleCompact}
      onHideNode={hideNode}
    />
    {#if saveWarnings.length > 0}
      <ul class="save-warnings">
        {#each saveWarnings as warning}
          <li>{warning}</li>
        {/each}
      </ul>
    {/if}
  </aside>

  {#if isMobile && sheetOpen && selectedNode}
    <div class="sheet-scrim" role="presentation" onclick={() => (sheetOpen = false)}></div>
    <div class="sheet" role="dialog" aria-label="Edit node">
      <div class="grip"><span></span></div>
      <div class="sheet-body">
        <NodePanel
          node={selectedNode}
          {nodes}
          {edges}
          {savedAt}
          onUpdateData={updateNodeData}
          onDeleteNode={deleteNode}
          onRenameNode={renameGraphNode}
          onRetypeNode={retypeNode}
          onApplyPreset={applyCurvePreset}
          onDuplicateNode={duplicateGraphNode}
          onRenameRow={renameDeviceRow}
          onToggleRow={toggleRowHidden}
          onToggleCompact={toggleCompact}
          onHideNode={hideNode}
          onClose={() => (sheetOpen = false)}
        />
      </div>
      <div class="sheet-foot">
        <button type="button" class="btn" onclick={() => (sheetOpen = false)}>Done</button>
        <button type="button" class="btn primary" disabled={saving} onclick={() => { sheetOpen = false; save(); }}>Apply to profile</button>
      </div>
    </div>
  {/if}

  {#if mobileView === 'canvas'}
    <button type="button" class="btn gale-view-toggle" onclick={() => (mobileView = 'chains')}>Chains</button>
  {/if}
</section>

<style>
  .save-warnings {
    margin: 0;
    padding-left: 1.1rem;
    color: var(--warn);
    font-size: 0.85rem;
  }

  .gale-chains {
    display: none;
  }

  .sheet-scrim,
  .sheet {
    display: none;
  }

  @media (max-width: 720px) {
    .gale-canvas.mobile-hidden {
      display: none;
    }

    .gale-chains {
      display: flex;
      flex-direction: column;
      gap: 18px;
      overflow: auto;
      padding: 16px 16px 110px;
      background: var(--canvas);
      background-image: radial-gradient(var(--grid) 1px, transparent 1px);
      background-size: 20px 20px;
    }

    .gale-chains.mobile-hidden {
      display: none;
    }

    .chains-head {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .chains-head h1 {
      font-size: 20px;
    }

    .chains-actions {
      margin-left: auto;
      display: flex;
      gap: 6px;
    }

    .chains-actions .btn {
      height: 36px;
      padding: 0 12px;
    }

    .chains-foot {
      display: flex;
      gap: 6px;
    }

    .chains-foot .btn {
      flex: 1;
      height: 44px;
      font-size: 13px;
    }

    .gale-view-toggle {
      display: block;
      position: absolute;
      top: 58px;
      left: 10px;
      z-index: 6;
      background: var(--surface);
    }

    .sheet-scrim {
      display: block;
      position: fixed;
      inset: 0;
      background: rgba(0, 0, 0, 0.55);
      z-index: 40;
    }

    .sheet {
      display: flex;
      flex-direction: column;
      position: fixed;
      left: 0;
      right: 0;
      bottom: 0;
      top: 72px;
      background: var(--surface);
      border: 1px solid var(--line);
      border-bottom: none;
      border-radius: 18px 18px 0 0;
      z-index: 41;
      overflow: hidden;
    }

    .grip {
      display: flex;
      justify-content: center;
      padding: 8px 0 0;
    }

    .grip span {
      width: 36px;
      height: 4px;
      border-radius: 2px;
      background: var(--line2);
    }

    .sheet-body {
      flex: 1;
      overflow: auto;
      padding: 14px 16px 24px;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }

    .sheet-foot {
      padding: 10px 16px calc(12px + env(safe-area-inset-bottom));
      border-top: 1px solid var(--line);
      background: var(--surface);
      display: flex;
      gap: 8px;
    }

    .sheet-foot .btn {
      height: 48px;
      font-size: 14px;
    }

    .sheet-foot .btn:first-child {
      flex: 1;
    }

    .sheet-foot .btn:last-child {
      flex: 2;
    }
  }
</style>
