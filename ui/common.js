// Shared by every Ebb window. The Rust core owns all state; windows only draw it
// and call commands. State arrives once a second on "ebb://state".

const T = window.__TAURI__;
export const invoke = T.core.invoke;
export const listen = T.event.listen;

/** Calls `draw(state)` now and on every update. */
export function onState(draw) {
  invoke('get_state').then(draw);
  listen('ebb://state', (e) => draw(e.payload));
}

/** 65 → "1:05", 3725 → "1:02:05". */
export function clock(seconds) {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = s % 60;
  const mm = h ? String(m).padStart(2, '0') : String(m);
  return (h ? `${h}:` : '') + `${mm}:${String(sec).padStart(2, '0')}`;
}

/** 3725 → "1 h 02 min", 300 → "5 min". */
export function hm(seconds) {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
  if (h) return `${h} h ${String(m).padStart(2, '0')} min`;
  if (m) return `${m} min`;
  return s ? '< 1 min' : '0 min';
}

export function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k.startsWith('on')) node.addEventListener(k.slice(2).toLowerCase(), v);
    else if (k === 'class') node.className = v;
    else if (k === 'html') node.innerHTML = v;
    else node.setAttribute(k, v === true ? '' : v);
  }
  for (const c of children.flat(Infinity)) {
    if (c == null || c === false) continue;
    node.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return node;
}

/** Title, colour and icon for the current phase. */
export function phaseInfo(s) {
  const p = s.phase;
  switch (p.kind) {
    case 'working': return { title: `Break in ${clock(s.secondsUntilBreak)}`, color: 'var(--tide)', icon: s.inMeeting ? 'video' : 'waves' };
    case 'warning': return { title: `Break in ${clock(s.secondsUntilBreak)}`, color: 'var(--warn)', icon: 'hourglass' };
    case 'idle': return { title: s.activity === 'locked' ? 'Paused: screen locked' : 'Paused: no activity', color: 'var(--muted)', icon: 'waves' };
    case 'away': return { title: 'Welcome back', color: 'var(--muted)', icon: 'moon' };
    case 'meetingHold': return { title: 'Break waiting for your meeting', color: 'var(--meet)', icon: 'video' };
    case 'postMeetingGrace': return { title: `Break in ${clock(p.remaining)}`, color: 'var(--warn)', icon: 'video' };
    case 'onBreak': return { title: `On break · ${clock(p.remaining)}`, color: 'var(--sun)', icon: 'cup' };
    case 'paused': return {
      title: p.until ? `Paused until ${new Date(p.until).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}` : 'Paused',
      color: 'var(--muted)', icon: 'pause',
    };
  }
  return { title: 'Ebb', color: 'var(--tide)', icon: 'waves' };
}

/** What Ebb sees you doing, so you can trust why the clock runs or stops. */
export function activityText(s) {
  const grace = s.prefs.passiveGraceMinutes;
  switch (s.activity) {
    case 'using': return null;
    case 'watching': return 'Watching a video: still counting as screen time';
    case 'reading': return `No input, still counting (up to ${grace} min). Reading or watching something?`;
    case 'locked': return 'Screen locked: the clock is paused';
    case 'gone': return 'You seem to be away';
  }
  return null;
}

const ICONS = {
  waves: '<path d="M3 9c2-2 4-2 6 0s4 2 6 0 4-2 6 0M3 14c2-2 4-2 6 0s4 2 6 0 4-2 6 0M3 19c2-2 4-2 6 0s4 2 6 0 4-2 6 0"/>',
  hourglass: '<path d="M6 3h12M6 21h12M7 3c0 5 10 5 10 9s-10 4-10 9M17 3c0 5-10 5-10 9s10 4 10 9"/>',
  moon: '<path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/>',
  video: '<rect x="2.5" y="6" width="13" height="12" rx="2.5"/><path d="M15.5 10.5 21 7v10l-5.5-3.5"/>',
  cup: '<path d="M4 9h13v5a5 5 0 0 1-5 5H9a5 5 0 0 1-5-5V9zM17 10h1.5a2.5 2.5 0 0 1 0 5H17M7 3.5c0 1.5 1 1.5 1 3M11 3.5c0 1.5 1 1.5 1 3"/>',
  pause: '<circle cx="12" cy="12" r="9"/><path d="M10 9v6M14 9v6"/>',
  eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  bolt: '<path d="M13 2 4 14h7l-1 8 9-12h-7l1-8z"/>',
  drop: '<path d="M12 3s7 7.5 7 12a7 7 0 0 1-14 0c0-4.5 7-12 7-12z"/>',
  person: '<circle cx="12" cy="5" r="2.5"/><path d="M12 8v7M8 22l4-7 4 7M6 11h12"/>',
  stretch: '<circle cx="12" cy="4.5" r="2"/><path d="M4 7l8 3 8-3M12 10v5l-3 7M12 15l3 7"/>',
  moonStar: '<path d="M18 15.5A7 7 0 1 1 8.5 6a5.5 5.5 0 0 0 9.5 9.5zM18 3v4M16 5h4"/>',
  chart: '<path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/>',
  gear: '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9 7 7M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1"/>',
  check: '<path d="m5 12.5 4.5 4.5L19 7"/>',
  heart: '<path d="M12 20s-8-5-8-11a4.5 4.5 0 0 1 8-2.8A4.5 4.5 0 0 1 20 9c0 6-8 11-8 11z"/>',
  update: '<path d="M12 3v12M7 10l5 5 5-5M4 20h16"/>',
  calendar: '<rect x="3" y="5" width="18" height="16" rx="2.5"/><path d="M3 10h18M8 3v4M16 3v4"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M5 19l1.5-1.5M17.5 6.5 19 5"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
};

export function icon(name, size = 18) {
  const span = el('span', { class: 'icon', 'aria-hidden': 'true' });
  span.innerHTML = `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${ICONS[name] || ''}</svg>`;
  return span;
}

/** Progress ring as an SVG string. */
export function ring(progress, color, size = 56, stroke = 6) {
  const r = (size - stroke) / 2, c = 2 * Math.PI * r;
  const off = c * (1 - Math.min(1, Math.max(0, progress)));
  return `<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" class="ring">
    <circle cx="${size / 2}" cy="${size / 2}" r="${r}" stroke="var(--track)" stroke-width="${stroke}" fill="none"/>
    <circle cx="${size / 2}" cy="${size / 2}" r="${r}" stroke="${color}" stroke-width="${stroke}" fill="none"
      stroke-linecap="round" stroke-dasharray="${c}" stroke-dashoffset="${off}" transform="rotate(-90 ${size / 2} ${size / 2})"/>
  </svg>`;
}

export const EXCEPTION_CATEGORIES = ['Deep work / in flow', 'Deadline', 'Joining a meeting', 'Finishing a call', 'Other'];

/**
 * "I'm in the middle of something." Asking costs a little friction on purpose:
 * pick how long, say why, and it comes out of a small daily budget.
 * Returns { node, update(state) }.
 */
export function exceptionForm({ onCancel, onDone }) {
  let minutes = null;
  let last = null;
  const choices = el('div', { class: 'chips' });
  const category = el('select', {}, EXCEPTION_CATEGORIES.map((c) => el('option', {}, c)));
  const reason = el('input', { type: 'text', maxlength: 140 });
  const message = el('p', { class: 'denial', hidden: true });
  const left = el('span', { class: 'muted small' });
  const grant = el('button', { class: 'primary' }, 'Grant me time');

  const reasonOK = () => !last?.prefs.requireExceptionReason || reason.value.trim().length >= 3;
  const refreshButton = () => {
    grant.disabled = minutes == null || !reasonOK();
    grant.textContent = minutes ? `Grant me ${minutes} min` : 'Grant me time';
  };
  reason.addEventListener('input', refreshButton);
  reason.addEventListener('keydown', (e) => { if (e.key === 'Enter' && !grant.disabled) grant.click(); });

  grant.addEventListener('click', async () => {
    try {
      await invoke('request_exception', { minutes, category: category.value, reason: reason.value });
      onDone?.();
    } catch (err) {
      message.hidden = false;
      message.textContent = String(err);
    }
  });

  const node = el('div', { class: 'exception-form' },
    el('div', { class: 'row between' }, el('h2', {}, 'Ask for more time'), left),
    el('label', { class: 'field' }, el('span', {}, 'How long?'), choices),
    el('label', { class: 'field' }, el('span', {}, 'Why?'), category, reason),
    message,
    el('div', { class: 'row between' },
      el('button', { class: 'ghost', onclick: () => onCancel?.() }, 'Never mind'),
      grant),
  );

  function update(s) {
    const first = last == null;
    last = s;
    left.textContent = `${s.exceptionsLeft} of ${s.prefs.exceptionsPerDay} left today`;
    reason.placeholder = s.prefs.requireExceptionReason
      ? "One line about what you're finishing (required)" : "One line about what you're finishing";
    const denials = new Map(s.exceptionDenials);
    const key = s.prefs.exceptionChoicesMinutes.join(',') + [...denials.values()].join('|');
    if (choices.dataset.key !== key) {
      choices.dataset.key = key;
      choices.replaceChildren(...s.prefs.exceptionChoicesMinutes.map((m) => {
        const b = el('button', { class: 'chip', disabled: denials.get(m) != null }, `${m} min`);
        b.addEventListener('click', () => {
          minutes = m;
          choices.querySelectorAll('.chip').forEach((c) => c.classList.toggle('on', c === b));
          refreshButton();
        });
        if (m === minutes) b.classList.add('on');
        return b;
      }));
      if (minutes != null && denials.get(minutes) != null) minutes = null;
    }
    let text = null;
    if (s.exceptionsLeft === 0) text = "You've used all of today's exceptions. Take the break: the work will still be there.";
    else if (s.maxExceptionMinutes < Math.min(...s.prefs.exceptionChoicesMinutes)) text = `You've hit the ${s.prefs.hardCapMinutes}-minute limit of continuous work.`;
    if (text) { message.hidden = false; message.textContent = text; }
    refreshButton();
    if (first) setTimeout(() => reason.focus(), 50);
  }

  return { node, update };
}
