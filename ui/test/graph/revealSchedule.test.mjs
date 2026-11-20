import { test } from 'node:test';
import assert from 'node:assert/strict';
import { allFinished, createRevealSchedule, layoutTransitions } from '../../src/lib/graph/revealSchedule.js';

function fakeTiming() {
  let nextId = 1;
  const frames = new Map();
  const delays = new Map();
  return {
    frame(callback) {
      frames.set(nextId, callback);
      return nextId++;
    },
    cancelFrame: (id) => frames.delete(id),
    delay(callback, ms) {
      delays.set(nextId, { callback, ms });
      return nextId++;
    },
    cancelDelay: (id) => delays.delete(id),
    tick() {
      const due = [...frames.values()];
      frames.clear();
      for (const callback of due) callback();
    },
    expire() {
      const due = [...delays.values()];
      delays.clear();
      for (const { callback } of due) callback();
    },
    pendingDelays: () => delays.size,
  };
}

function deferred() {
  let resolve;
  const promise = new Promise((done) => (resolve = done));
  return { promise, resolve };
}

const flush = () => new Promise((done) => setImmediate(done));

function setup() {
  const timing = fakeTiming();
  let reveals = 0;
  const schedule = createRevealSchedule(() => (reveals += 1), timing, 1000);
  return { timing, schedule, reveals: () => reveals };
}

test('with no running transitions the reveal lands two frames after the request', async () => {
  const { timing, schedule, reveals } = setup();
  schedule.request();
  await flush();
  timing.tick();
  assert.equal(reveals(), 0);
  timing.tick();
  assert.equal(reveals(), 1);
  assert.equal(timing.pendingDelays(), 0);
});

test('the reveal waits for a running layout transition however many frames it takes', async () => {
  const { timing, schedule, reveals } = setup();
  const transition = deferred();
  schedule.request(transition.promise);
  await flush();
  for (let frame = 0; frame < 10; frame += 1) timing.tick();
  assert.equal(reveals(), 0);
  transition.resolve();
  await flush();
  timing.tick();
  timing.tick();
  assert.equal(reveals(), 1);
});

test('a resize after the transition restarts the layout wait so the final size is measured', async () => {
  const { timing, schedule, reveals } = setup();
  schedule.request();
  await flush();
  timing.tick();
  schedule.resized();
  timing.tick();
  assert.equal(reveals(), 0);
  timing.tick();
  assert.equal(reveals(), 1);
});

test('resizes before the transition settles or after the reveal do nothing', async () => {
  const { timing, schedule, reveals } = setup();
  const transition = deferred();
  schedule.request(transition.promise);
  schedule.resized();
  timing.tick();
  timing.tick();
  assert.equal(reveals(), 0);
  transition.resolve();
  await flush();
  timing.tick();
  timing.tick();
  schedule.resized();
  timing.tick();
  timing.tick();
  assert.equal(reveals(), 1);
});

test('a transition that never finishes is cut off by the settle limit', async () => {
  const { timing, schedule, reveals } = setup();
  schedule.request(new Promise(() => {}));
  timing.expire();
  timing.tick();
  timing.tick();
  assert.equal(reveals(), 1);
});

test('a newer request replaces an older one still waiting', async () => {
  const { timing, schedule, reveals } = setup();
  const first = deferred();
  schedule.request(first.promise);
  schedule.request();
  first.resolve();
  await flush();
  timing.expire();
  timing.tick();
  timing.tick();
  timing.tick();
  timing.tick();
  assert.equal(reveals(), 1);
});

test('cancel drops a pending reveal', async () => {
  const { timing, schedule, reveals } = setup();
  schedule.request();
  await flush();
  timing.tick();
  schedule.cancel();
  timing.tick();
  timing.expire();
  timing.tick();
  timing.tick();
  assert.equal(reveals(), 0);
});

function fakeElement(animations, parentElement = null) {
  return { getAnimations: () => animations, parentElement };
}

test('layout transitions are the running CSS transitions on the element and its ancestors', () => {
  const grid = { playState: 'running', transitionProperty: 'grid-template-columns' };
  const done = { playState: 'finished', transitionProperty: 'width' };
  const keyframes = { playState: 'running', animationName: 'fade-in' };
  const page = fakeElement([grid, keyframes]);
  const canvas = fakeElement([done], page);
  const flow = fakeElement([], canvas);
  assert.deepEqual(layoutTransitions(flow), [grid]);
  assert.deepEqual(layoutTransitions(null), []);
});

test('allFinished resolves even when a transition is cancelled', async () => {
  const cancelled = { finished: Promise.reject(new Error('AbortError')) };
  const finished = { finished: Promise.resolve('done') };
  assert.deepEqual(await allFinished([cancelled, finished]), [null, 'done']);
});
