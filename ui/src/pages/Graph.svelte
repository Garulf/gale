<script>
  import { onMount, onDestroy, setContext } from 'svelte';
  import { SvelteFlow, Background, Controls } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import '../lib/graph/styles.css';
  import { getConfig, putConfig, getInventory } from '../lib/api.js';
  import { warnings, refreshWarnings } from '../lib/warnings.js';
  import { page } from '../lib/page.js';
  import { configToGraph, graphToConfig, edgeInto, replaceEdge, adoptSavedTokens } from '../lib/graph/model.js';
  import { isValidConnection } from '../lib/graph/validate.js';
  import { autoLayout } from '../lib/graph/layout.js';
  import { nodeKind, edgeId } from '../lib/graph/ids.js';
  import { defaultVirtualSensor } from '../lib/sensors.js';
  import { defaultCurve } from '../lib/graph/defaults.js';
  import { configValidationError } from '../lib/graph/configValidation.js';
  import { collectNodeWarnings } from '../lib/graph/nodeWarnings.js';
  import { renameNode, changeVirtualType, changeCurveType, nameInUse } from '../lib/graph/edit.js';
  import DeviceSensorNode from '../lib/graph/components/DeviceSensorNode.svelte';
  import DeviceControlNode from '../lib/graph/components/DeviceControlNode.svelte';
  import VirtualNode from '../lib/graph/components/VirtualNode.svelte';
  import CurveNode from '../lib/graph/components/CurveNode.svelte';
  import CombineNode from '../lib/graph/components/CombineNode.svelte';
  import GaleEdge from '../lib/graph/components/GaleEdge.svelte';
  import NodePanel from '../lib/graph/components/NodePanel.svelte';
  import GraphToolbar from '../lib/graph/components/GraphToolbar.svelte';

  let config = $state.raw(null);
  let inventory = $state.raw(null);
  let editingProfile = $state('');
  let nodes = $state.raw([]);
  let edges = $state.raw([]);
  let selectedNodeId = $state('');
  let dirty = $state(false);
  let history = $state([]);
  let future = $state([]);
  let showEdgeLabels = $state(true);
  let error = $state('');
  let saveWarnings = $state([]);
  let saving = $state(false);
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

  let nodeWarnings = $derived(collectNodeWarnings(nodes, edges, [...$warnings, ...saveWarnings]));
  setContext('galeNodeWarnings', () => nodeWarnings);

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
    try {
      const [cfg, inv] = await Promise.all([getConfig(), getInventory()]);
      config = cfg;
      inventory = inv;
      editingProfile = cfg.active_profile;
      const graph = configToGraph(config, inventory, editingProfile);
      nodes = graph.nodes;
      edges = toFlowEdges(graph.edges, showEdgeLabels);
      selectedNodeId = '';
      history = [];
      future = [];
      dirty = false;
    } catch (err) {
      error = err.message;
    }
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

  function deleteNode(id) {
    snapshotHistory();
    edges = edges.filter((edge) => edge.source !== id && edge.target !== id);
    nodes = nodes.filter((node) => node.id !== id);
    if (selectedNodeId === id) selectedNodeId = '';
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
    dirty = true;
    future = [];
  }

  function addVirtualNode(position) {
    const name = uniqueVirtualName();
    appendNode({
      id: `virtual:${name}`,
      type: 'virtual',
      position: cascadePosition(position),
      hidden: false,
      data: { virtual: { name, config: defaultVirtualSensor('max') } },
    });
  }

  function addCurveNode(position) {
    const id = uniqueCurveId('curve');
    appendNode({
      id: `curve:${id}`,
      type: 'curve',
      position: cascadePosition(position),
      hidden: false,
      data: { curve: { id, config: defaultCurve('point') } },
    });
  }

  function addCombineNode(position) {
    const id = uniqueCurveId('combine');
    appendNode({
      id: `combine:${id}`,
      type: 'combine',
      position: cascadePosition(position),
      hidden: false,
      data: { combine: { id, config: defaultCurve('mix') } },
    });
  }

  async function save() {
    saving = true;
    error = '';
    saveWarnings = [];
    const wireConfig = graphToConfig(nodes, edges, config, editingProfile);
    const invalid = configValidationError(wireConfig);
    if (invalid) {
      error = invalid;
      saving = false;
      return;
    }
    try {
      const result = await putConfig(wireConfig);
      saveWarnings = (result && result.warnings) || [];
      const saved = await getConfig();
      config = saved;
      nodes = adoptSavedTokens(nodes, saved.profiles[editingProfile]);
      dirty = false;
      await refreshWarnings();
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
</script>

<section class="gale-graph-page">
  <div class="gale-canvas">
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
      fitView
    >
      <Background />
      <Controls />
      <GraphToolbar
        {showEdgeLabels}
        canUndo={history.length > 0}
        canRedo={future.length > 0}
        {saving}
        onAutoLayout={runAutoLayout}
        onUndo={undo}
        onRedo={redo}
        onSave={save}
        onDiscard={discard}
        onToggleLabels={toggleEdgeLabels}
        onAddVirtual={addVirtualNode}
        onAddCurve={addCurveNode}
        onAddCombine={addCombineNode}
      />
    </SvelteFlow>

    {#if hiddenNodes.length > 0}
      <div class="gale-hidden-strip">
        {#each hiddenNodes as node (node.id)}
          <div class="gale-hidden-chip">
            <span>{node.type === 'deviceSensor' ? node.data.deviceSensor.device : node.data.deviceControl.device}</span>
            <button type="button" onclick={() => showNode(node.id)}>Show</button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <aside class="gale-panel">
    {#if error}
      <p class="error">{error}</p>
    {/if}
    <NodePanel
      node={selectedNode}
      {edges}
      onUpdateData={updateNodeData}
      onDeleteNode={deleteNode}
      onRenameNode={renameGraphNode}
      onRetypeNode={retypeNode}
    />
    {#if dirty}
      <div class="warn">Unsaved changes on this profile. Saving applies them to the daemon and validates the whole graph.</div>
    {/if}
    {#if saveWarnings.length > 0}
      <ul class="save-warnings">
        {#each saveWarnings as warning}
          <li>{warning}</li>
        {/each}
      </ul>
    {/if}
  </aside>
</section>

<style>
  .error {
    color: #f87171;
  }

  .warn {
    border: 1px solid #f59e0b55;
    background: #f59e0b12;
    color: #f5c16b;
    border-radius: 6px;
    padding: 8px 10px;
    font-size: 12px;
  }

  .save-warnings {
    margin: 0;
    padding-left: 1.1rem;
    color: #fbbf24;
    font-size: 0.85rem;
  }
</style>
