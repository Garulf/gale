export const SETTLE_LIMIT_MS = 1000;

const browserTiming = {
  frame: (callback) => requestAnimationFrame(callback),
  cancelFrame: (id) => cancelAnimationFrame(id),
  delay: (callback, ms) => setTimeout(callback, ms),
  cancelDelay: (id) => clearTimeout(id),
};

export function layoutTransitions(element) {
  const running = [];
  for (let node = element; node; node = node.parentElement) {
    for (const animation of node.getAnimations()) {
      if (animation.playState === 'running' && 'transitionProperty' in animation) running.push(animation);
    }
  }
  return running;
}

export function allFinished(animations) {
  return Promise.all(animations.map((animation) => animation.finished.catch(() => null)));
}

export function createRevealSchedule(reveal, timing = browserTiming, settleLimitMs = SETTLE_LIMIT_MS) {
  let generation = 0;
  let settled = false;
  let frameId = null;
  let limitId = null;

  function clearTimers() {
    if (frameId !== null) timing.cancelFrame(frameId);
    if (limitId !== null) timing.cancelDelay(limitId);
    frameId = null;
    limitId = null;
  }

  function revealAfterLayout() {
    if (frameId !== null) timing.cancelFrame(frameId);
    frameId = timing.frame(() => {
      frameId = timing.frame(() => {
        frameId = null;
        if (limitId !== null) timing.cancelDelay(limitId);
        limitId = null;
        generation += 1;
        reveal();
      });
    });
  }

  function settle(current) {
    if (current !== generation || settled) return;
    settled = true;
    revealAfterLayout();
  }

  return {
    request(transitionsDone = Promise.resolve()) {
      clearTimers();
      generation += 1;
      settled = false;
      const current = generation;
      limitId = timing.delay(() => settle(current), settleLimitMs);
      transitionsDone.then(() => settle(current));
    },
    resized() {
      if (settled && frameId !== null) revealAfterLayout();
    },
    cancel() {
      clearTimers();
      generation += 1;
    },
  };
}
