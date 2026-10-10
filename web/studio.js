'use strict';
const $ = id => document.getElementById(id);
const terminal = stage => ['completed', 'failed', 'cancelled'].includes(stage);
const stages = ['exporting', 'compiling', 'simulating', 'analysing'];
const stageNames = {queued: 'Queued', exporting: 'Preparing', compiling: 'Compiling', simulating: 'Running', analysing: 'Inspecting', completed: 'Completed', failed: 'Failed', cancelled: 'Cancelled'};
const jobs = new Map();
const activeJobs = new Set();
const pollingJobs = new Set();
let upload = null, selectedJob = null, submissionPending = false;

function showError(message) { $('upload-error').textContent = message; $('upload-error').hidden = !message; }
function updateRunButton() { $('run').disabled = !upload || Boolean(activeJobs.size) || submissionPending; }
function setUpload(filename, source, bytes) {
  upload = {filename, source};
  $('filename').textContent = filename;
  $('file-size').textContent = `${(bytes / 1024).toFixed(1)} KiB`;
  $('source-preview').textContent = source;
  $('source-meta').hidden = false; $('source-preview').hidden = false;
  showError(''); updateRunButton();
}
async function readFile(file) {
  try {
    if (!file || !file.name.endsWith('.blog')) throw new Error('Choose a .blog file exported by Bloq.');
    if (!file.size || file.size > 262144) throw new Error('Choose a nonempty BLOG file no larger than 256 KiB.');
    const source = new TextDecoder('utf-8', {fatal: true}).decode(await file.arrayBuffer());
    if (!source.trimStart().startsWith('BLOG ')) throw new Error('This file has no BLOG document header.');
    setUpload(file.name, source, file.size);
  } catch (error) { showError(error.message); }
}
async function api(url, options) {
  const response = await fetch(url, options);
  if (response.status === 204) return null;
  const body = await response.json();
  if (!response.ok) throw new Error(body.error?.message || `Request failed (${response.status}).`);
  return body;
}
function node(tag, text, parent, className) {
  const element = document.createElement(tag);
  if (text !== undefined) element.textContent = text;
  if (className) element.className = className;
  parent.append(element); return element;
}
function formatTime(ns) {
  if (ns >= 1e6) return `${(ns / 1e6).toFixed(3)} ms`;
  if (ns >= 1e3) return `${(ns / 1e3).toFixed(2)} µs`;
  return `${ns} ns`;
}
function renderMetrics(report) {
  const m = report.metrics;
  $('metrics').replaceChildren(); $('outputs').replaceChildren();
  $('metrics').hidden = !m; $('result-detail').hidden = !m;
  if (!m) return;
  for (const [label, value, note] of [
    ['Simulated time', formatTime(m.simulated_duration_ns), `Stop tick: ${m.stop_tick_ns.toLocaleString()} ns`],
    ['Retired instructions', m.retired_instructions.toLocaleString(), `Last CPU edge index: ${m.last_cpu_cycle ?? '—'}`],
    ['Physical operations', m.physical_operations.toLocaleString(), `${m.timing_points} triggered time points`],
    ['Measurement samples', m.measurement_samples.toLocaleString(), 'Includes reset measurements'],
    ['Physical qubits', m.physical_qubits, `Timing queue peak: ${m.timing_queue_peak}`],
    ['Host duration', `${(report.host_duration_ms / 1000).toFixed(2)} s`, 'Includes queue, compile, run, and analysis'],
  ]) { const card = node('div', undefined, $('metrics'), 'metric'); node('span', label, card); node('strong', String(value), card); node('small', note, card); }
  const latency = m.measurement_delivery;
  $('delivery').textContent = latency.samples ? `Mean delivery: ${latency.mean_ns.toFixed(1)} ns · max ${latency.max_ns} ns` : 'No delivery latency samples';
  $('output-note').textContent = report.backend === 'mock' ? 'Mock bits exercise controller behavior. They are not quantum-state simulation results.' : 'Measurement records and raw observables from one ideal shot. Decoder corrections are not applied.';
  for (const output of m.outputs.slice(0, 256)) { const row = node('tr', undefined, $('outputs')); node('td', output.label, row); node('td', String(output.value), row); }
  if (!m.outputs.length) { const row = node('tr', undefined, $('outputs')); const cell = node('td', 'No accepted output bits.', row); cell.colSpan = 2; }
  if (m.outputs.length > 256) $('output-note').textContent += ' Showing the first 256 outputs; download metrics.json for all bits.';
}
function renderHistory() {
  $('history').hidden = !jobs.size; $('history-list').replaceChildren();
  for (const report of jobs.values()) {
    const button = node('button', `${report.filename} · ${stageNames[report.stage]}`, $('history-list'));
    button.type = 'button'; button.setAttribute('aria-pressed', String(report.id === selectedJob));
    button.addEventListener('click', () => { selectedJob = report.id; render(report); });
  }
}
function render(report) {
  $('empty-state').hidden = true; $('run-status').hidden = false;
  $('run-name').textContent = report.filename;
  $('run-description').textContent = `Distance ${report.distance} · seed ${report.seed} · ${report.backend} · one shot`;
  $('status-badge').textContent = stageNames[report.stage]; $('status-badge').dataset.stage = report.stage;
  $('cancel').hidden = terminal(report.stage); $('cancel').disabled = false;
  const stage = report.failed_stage || report.stage;
  const index = stages.indexOf(stage);
  for (const item of $('stages').children) {
    const position = stages.indexOf(item.dataset.stage);
    item.classList.toggle('done', report.stage === 'completed' || (index >= 0 && position < index));
    item.classList.toggle('active', !terminal(report.stage) && position === index);
  }
  $('run-error').hidden = !report.error; $('run-error').textContent = report.error || '';
  renderMetrics(report);
  $('artifact-links').replaceChildren(); $('downloads').hidden = !report.artifacts.length;
  for (const name of report.artifacts) {
    const link = node('a', name, $('artifact-links'));
    link.href = `/api/jobs/${report.id}/artifacts/${encodeURIComponent(name)}`;
    link.setAttribute('download', name);
  }
  $('remove-run').hidden = !terminal(report.stage);
  $('replay-section').hidden = !report.replay_ready;
  if (report.replay_ready) {
    const url = `/runs/${report.id}/player`;
    if ($('replay').getAttribute('src') !== url) $('replay').src = url;
    $('open-replay').href = url;
  }
  renderHistory();
}
async function poll(id) {
  if (pollingJobs.has(id)) return;
  pollingJobs.add(id);
  let retries = 0;
  while (true) {
    try {
      const report = await api(`/api/jobs/${id}`);
      retries = 0; jobs.set(id, report);
      if (selectedJob === id) render(report); else renderHistory();
      if (terminal(report.stage)) { activeJobs.delete(id); pollingJobs.delete(id); updateRunButton(); return; }
    } catch (error) {
      if (++retries >= 5) {
        // Keep the active ID: a failed HTTP request does not cancel a server job.
        showError(`Run status is unavailable: ${error.message} Reloading starts a new browser view; the run may still be active.`);
        pollingJobs.delete(id); return;
      }
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
}
$('file-input').addEventListener('change', event => readFile(event.target.files[0]));
$('dropzone').addEventListener('dragover', event => { event.preventDefault(); $('dropzone').classList.add('dragover'); });
$('dropzone').addEventListener('dragleave', () => $('dropzone').classList.remove('dragover'));
$('dropzone').addEventListener('drop', event => { event.preventDefault(); $('dropzone').classList.remove('dragover'); readFile(event.dataTransfer.files[0]); });
$('clear-file').addEventListener('click', () => { upload = null; $('file-input').value = ''; $('source-meta').hidden = true; $('source-preview').hidden = true; updateRunButton(); });
$('backend').addEventListener('change', () => { $('backend-note').textContent = $('backend').value === 'mock' ? 'Predetermined outcomes for inspecting controller behavior.' : 'Ideal Clifford evolution, without injected noise.'; });
$('example').addEventListener('click', async () => {
  try { const response = await fetch('/examples/x-memory.blog'); if (!response.ok) throw new Error('Example could not be loaded.'); const source = await response.text(); setUpload('x-memory.blog', source, new TextEncoder().encode(source).length); } catch (error) { showError(error.message); }
});
$('run-form').addEventListener('submit', async event => {
  event.preventDefault(); if (!upload || activeJobs.size || submissionPending) return;
  const seed = Number($('seed').value);
  if (!Number.isInteger(seed) || seed < 0 || seed > 4294967295) { showError('Seed must be an integer from 0 to 4294967295.'); return; }
  submissionPending = true; updateRunButton(); showError('');
  try {
    const report = await api('/api/jobs', {method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({...upload, distance:Number($('distance').value), seed, backend:$('backend').value})});
    jobs.set(report.id, report); activeJobs.add(report.id); selectedJob = report.id;
    render(report); updateRunButton(); void poll(report.id);
  } catch (error) { showError(error.message); }
  finally { submissionPending = false; updateRunButton(); }
});
$('cancel').addEventListener('click', async () => {
  $('cancel').disabled = true;
  try { await api(`/api/jobs/${selectedJob}/cancel`, {method:'POST'}); } catch (error) { showError(error.message); $('cancel').disabled = false; }
});
$('remove-run').addEventListener('click', async () => {
  try {
    await api(`/api/jobs/${selectedJob}`, {method:'DELETE'});
    jobs.delete(selectedJob); $('replay').removeAttribute('src');
    const remaining = [...jobs.values()].at(-1);
    selectedJob = remaining?.id ?? null;
    if (remaining) render(remaining);
    else { $('run-status').hidden = true; $('empty-state').hidden = false; $('replay-section').hidden = true; $('status-badge').textContent = 'Ready when you are'; delete $('status-badge').dataset.stage; renderHistory(); }
  } catch (error) { showError(error.message); }
});
$('replay').addEventListener('load', () => {
  const frame = $('replay');
  if (!frame.getAttribute('src')) return;
  try {
    const root = frame.contentDocument.getElementById('trace-player');
    if (!root) return;
    const resize = () => { frame.style.height = `${Math.min(2600, Math.max(700, root.scrollHeight + 36))}px`; };
    new ResizeObserver(resize).observe(root); resize();
  } catch { /* Standalone replay remains usable if frame sizing is unavailable. */ }
});
async function restoreSession() {
  try {
    const info = await api('/api/health');
    $('player-version').textContent = `qsbit-sim player · ${info.player_revision.slice(0, 7)}`;
    const history = await api('/api/jobs');
    for (const report of history.jobs) { jobs.set(report.id, report); if (!terminal(report.stage)) activeJobs.add(report.id); }
    // A user can submit before the history request returns. Preserve that view.
    if (!selectedJob && jobs.size) { const latest = [...jobs.values()].at(-1); selectedJob = latest.id; render(latest); }
    updateRunButton();
    for (const id of activeJobs) void poll(id);
  } catch (error) { showError(`Studio service is unavailable: ${error.message}`); }
}
void restoreSession();
