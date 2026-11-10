// Runs after app.js: shows the live meter's "recovered" readout for the demo sweep.
setTimeout(() => {
  const d = window.__DEMO__;
  startRecovery(d.before, d.sweepT, d.estimate, false);
  recovery.until = 0; // measuring window is over
  renderLive();
}, 400);
