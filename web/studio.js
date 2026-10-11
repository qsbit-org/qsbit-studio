'use strict';
const $ = id => document.getElementById(id);
const terminal = stage => ['completed', 'failed', 'cancelled'].includes(stage);
const stages = ['exporting', 'compiling', 'simulating', 'analysing'];
const stageNames = {queued: 'Queued', exporting: 'Preparing', compiling: 'Compiling', simulating: 'Running', analysing: 'Inspecting', completed: 'Completed', failed: 'Failed', cancelled: 'Cancelled'};
const jobs = new Map();
const activeJobs = new Set();
const pollingJobs = new Set();
let upload = null, selectedJob = null, submissionPending = false, uploadGeneration = 0, logGeneration = 0;
const connections = new Map();

function showError(message) { $('upload-error').textContent = message; $('upload-error').hidden = !message; }
function updateRunButton() { $('run').disabled = !upload || Boolean(activeJobs.size) || submissionPending; }
function setUpload(filename, source, bytes) {
  upload = {filename, source};
  $('filename').textContent = filename;
  $('file-size').textContent = `${(bytes / 1024).toFixed(1)} KiB`;
  $('source-preview').textContent = source;
  $('source-meta').hidden = false; $('source-details').hidden = false;
  $('file-actions').hidden = false; $('dropzone').hidden = true;
  showError(''); updateRunButton();
}
async function readFile(file) {
  const generation = ++uploadGeneration;
  try {
    if (!file || !file.name.endsWith('.blog')) throw new Error('Choose a .blog file exported by Bloq.');
    if (!file.size || file.size > 262144) throw new Error('Choose a nonempty BLOG file no larger than 256 KiB.');
    const source = new TextDecoder('utf-8', {fatal: true}).decode(await file.arrayBuffer());
    if (generation !== uploadGeneration) return;
    if (!source.trimStart().startsWith('BLOG ')) throw new Error('This file has no BLOG document header.');
    setUpload(file.name, source, file.size);
  } catch (error) { if (generation === uploadGeneration) showError(error.message); }
}
async function api(url, options) {
  const response = await fetch(url, {signal: AbortSignal.timeout(10000), ...options});
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
  ]) { const card = node('div', undefined, $('metrics'), 'metric'); node('span', label, card); node('strong', String(value), card); node('small', note, card); card.title = note; }
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
    button.addEventListener('click', () => { selectJob(report.id); });
  }
}
function selectJob(id) {
  if (selectedJob !== id) { logGeneration++; $('log-content').textContent = 'Select a log when the run finishes.'; }
  selectedJob = id; render(jobs.get(id));
}
function connectionState() {
  const message = connections.get(selectedJob) || '';
  $('connection-status').textContent = message;
  $('retry-connection').hidden = !message || pollingJobs.has(selectedJob);
}
function render(report) {
  connectionState();
  $('empty-state').hidden = true; $('run-status').hidden = false;
  $('run-name').textContent = report.filename;
  $('run-description').textContent = `Target ${report.target} · distance ${report.distance} · seed ${report.seed} · ${report.backend} · one shot`;
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
  $('replay-empty').hidden = report.replay_ready;
  if (!report.replay_ready) $('replay').removeAttribute('src');
  $('log-links').replaceChildren();
  for (const name of report.artifacts.filter(name => name.endsWith('.log'))) {
    const button = node('button', name, $('log-links')); button.type = 'button';
    button.addEventListener('click', () => loadLog(report.id, name));
  }
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
      retries = 0; connections.delete(id); jobs.set(id, report);
      if (selectedJob === id) render(report); else renderHistory();
      if (terminal(report.stage)) { activeJobs.delete(id); pollingJobs.delete(id); updateRunButton(); return; }
    } catch (error) {
      connections.set(id, 'Connection lost. Retrying…'); if (selectedJob === id) connectionState();
      if (++retries >= 5) {
        // Keep the active ID: a failed HTTP request does not cancel a server job.
        connections.set(id, `Status unavailable: ${error.message}`);
        pollingJobs.delete(id); if (selectedJob === id) connectionState(); return;
      }
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
}
$('file-input').addEventListener('change', event => readFile(event.target.files[0]));
$('dropzone').addEventListener('dragover', event => { event.preventDefault(); $('dropzone').classList.add('dragover'); });
$('dropzone').addEventListener('dragleave', () => $('dropzone').classList.remove('dragover'));
$('dropzone').addEventListener('drop', event => { event.preventDefault(); $('dropzone').classList.remove('dragover'); readFile(event.dataTransfer.files[0]); });
$('replace-file').addEventListener('click', () => $('file-input').click());
$('clear-file').addEventListener('click', () => {
  uploadGeneration++; upload = null; $('file-input').value = '';
  $('source-meta').hidden = $('source-details').hidden = $('file-actions').hidden = true;
  $('dropzone').hidden = false; showError(''); updateRunButton();
});
$('retry-connection').addEventListener('click', () => { if (selectedJob) void poll(selectedJob); });
$('backend').addEventListener('change', () => { $('backend-note').textContent = $('backend').value === 'mock' ? 'Predetermined outcomes for inspecting controller behavior.' : 'Ideal Clifford evolution, without injected noise.'; });
$('example').addEventListener('click', async () => {
  const generation = ++uploadGeneration;
  try { const response = await fetch('/examples/x-memory.blog'); if (!response.ok) throw new Error('Example could not be loaded.'); const source = await response.text(); if (generation !== uploadGeneration) return; setUpload('x-memory.blog', source, new TextEncoder().encode(source).length); } catch (error) { if (generation === uploadGeneration) showError(error.message); }
});
$('run-form').addEventListener('submit', async event => {
  event.preventDefault(); if (!upload || activeJobs.size || submissionPending) return;
  const seed = Number($('seed').value);
  if (!Number.isInteger(seed) || seed < 0 || seed > 4294967295) { showError('Seed must be an integer from 0 to 4294967295.'); return; }
  submissionPending = true; updateRunButton(); showError('');
  try {
    const report = await api('/api/jobs', {method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({...upload, distance:Number($('distance').value), seed, backend:$('backend').value, target:$('target').value || undefined})});
    jobs.set(report.id, report); activeJobs.add(report.id); selectJob(report.id);
    selectTab($('tab-replay')); updateRunButton(); void poll(report.id);
  } catch (error) { showError(error.message); }
  finally { submissionPending = false; updateRunButton(); }
});
$('cancel').addEventListener('click', async () => {
  const id = selectedJob; $('cancel').disabled = true;
  try { await api(`/api/jobs/${id}/cancel`, {method:'POST'}); } catch (error) { if (selectedJob === id) { showError(error.message); $('cancel').disabled = false; } }
});
$('remove-run').addEventListener('click', async () => {
  const id = selectedJob;
  try {
    await api(`/api/jobs/${id}`, {method:'DELETE'});
    jobs.delete(id); connections.delete(id);
    if (selectedJob !== id) { renderHistory(); return; }
    logGeneration++; $('replay').removeAttribute('src');
    const remaining = [...jobs.values()].at(-1);
    if (remaining) selectJob(remaining.id);
    else { selectedJob = null; $('run-status').hidden = true; $('empty-state').hidden = false; $('replay-section').hidden = true; $('status-badge').textContent = 'Ready'; delete $('status-badge').dataset.stage; connectionState(); renderHistory(); }
  } catch (error) { if (selectedJob === id) showError(error.message); }
});
const tabs = [...document.querySelectorAll('[role=tab]')];
function selectTab(tab) {
  tabs.forEach(item => {
    const selected = item === tab; item.setAttribute('aria-selected', String(selected)); item.tabIndex = selected ? 0 : -1;
    $(item.getAttribute('aria-controls')).hidden = !selected;
  });
}
tabs.forEach((tab, i) => {
  tab.addEventListener('click', () => selectTab(tab));
  tab.addEventListener('keydown', event => {
    let next;
    if (event.key === 'ArrowRight') next = (i + 1) % tabs.length;
    if (event.key === 'ArrowLeft') next = (i + tabs.length - 1) % tabs.length;
    if (event.key === 'Home') next = 0;
    if (event.key === 'End') next = tabs.length - 1;
    if (next !== undefined) { event.preventDefault(); tabs[next].focus(); selectTab(tabs[next]); }
  });
});
async function loadLog(id, name) {
  const generation = ++logGeneration;
  $('log-content').textContent = 'Loading…';
  try {
    const response = await fetch(`/api/jobs/${id}/artifacts/${encodeURIComponent(name)}`, {signal: AbortSignal.timeout(10000)});
    if (!response.ok) throw new Error(`Cannot load log (${response.status}).`);
    const text = await response.text();
    if (selectedJob === id && generation === logGeneration) $('log-content').textContent = text;
  } catch (error) {
    if (selectedJob === id && generation === logGeneration) $('log-content').textContent = error.message;
  }
}
function syncTheme() {
  if ($('replay').getAttribute('src')) $('replay').contentWindow.postMessage({type:'qsbit-theme', theme: window.QsbitTheme.get()}, location.origin);
}
$('replay').addEventListener('load', syncTheme);
window.addEventListener('qsbit-theme-change', syncTheme);
async function restoreSession() {
  try {
    const info = await api('/api/health');
    $('player-version').textContent = `qsbit-sim player · ${info.player_revision.slice(0, 7)}`;
    for (const id of info.targets) {
      const option = document.createElement('option'); option.value = id; option.textContent = id;
      $('target').append(option);
    }
    $('target').value = info.default_target; $('target').disabled = false;
    const history = await api('/api/jobs');
    for (const report of history.jobs) { jobs.set(report.id, report); if (!terminal(report.stage)) activeJobs.add(report.id); }
    // A user can submit before the history request returns. Preserve that view.
    if (!selectedJob && jobs.size) { const latest = [...jobs.values()].at(-1); selectedJob = latest.id; render(latest); }
    updateRunButton();
    for (const id of activeJobs) void poll(id);
  } catch (error) { showError(`Studio service is unavailable: ${error.message}`); }
}
void restoreSession();
