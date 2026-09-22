(() => {
  'use strict';
  const root = document.querySelector('.mobile-landing');
  if (!root) return;
  const $ = id => document.getElementById(id);
  const video = $('mobile-film');
  const playButton = $('mobile-play');
  const soundButton = $('mobile-sound');
  const filmStatus = $('mobile-film-status');
  const motion = matchMedia('(prefers-reduced-motion: reduce)');
  const pointer = matchMedia('(pointer: coarse)');
  const connection = navigator.connection;
  const active = () => document.documentElement.dataset.experience === 'mobile';
  let userPaused = motion.matches || Boolean(connection?.saveData);
  let visible = false;
  let hydrated = false;
  let playRequest = 0;
  let heardOpening = false;
  const notes = JSON.parse($('mobile-notes').textContent).events;
  const keyboard = [...root.querySelectorAll('.m-key')].map(key => ({
    key,
    rect: key.querySelector('rect'),
    label: key.querySelector('text'),
    color: getComputedStyle(key).color.match(/\d+/g).map(Number),
    notes: notes.filter(note => note.key === key.dataset.key),
    lit: -1
  }));
  let frameCallback = null;
  const videoFrames = typeof video.requestVideoFrameCallback === 'function';

  // Use presented video time, not timers. Seeking, buffering, pausing and
  // looping keep the keyboard on the same frame as the recorded note hit.
  function drawKeyboard(time) {
    for (const key of keyboard) {
      let lit = 0;
      for (const note of key.notes) {
        if (note.time > time + 0.000001) break;
        const release = Math.max(note.time + 0.07, note.until);
        lit = Math.max(lit, Math.min(1, 1 - (time - release) / 0.12));
      }
      lit = Math.round(Math.max(0, lit) * 100) / 100;
      if (lit === key.lit) continue;
      key.lit = lit;
      key.key.dataset.lit = String(lit);
      key.rect.style.fillOpacity = 0.14 + 0.30 * lit;
      key.rect.style.strokeOpacity = 0.45 + 0.50 * lit;
      key.label.style.fillOpacity = 0.85 + 0.15 * lit;
      key.label.style.fill = `rgb(${key.color.map(c => Math.round(c + (255 - c) * lit * 0.85)).join(',')})`;
    }
    const stamp = seconds => `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`;
    $('mobile-time').textContent = `${stamp(time)} / ${stamp(Number.isFinite(video.duration) ? Math.round(video.duration) : 24)}`;
  }
  function followVideo() {
    if (frameCallback !== null || video.paused || video.ended) return;
    const draw = (_, metadata) => {
      frameCallback = null;
      drawKeyboard(metadata ? metadata.mediaTime : video.currentTime);
      followVideo();
    };
    frameCallback = videoFrames ? video.requestVideoFrameCallback(draw) : requestAnimationFrame(draw);
  }
  function stopFollowing() {
    if (frameCallback === null) return;
    if (videoFrames) video.cancelVideoFrameCallback(frameCallback);
    else cancelAnimationFrame(frameCallback);
    frameCallback = null;
  }

  function hydrate() {
    if (hydrated) return;
    hydrated = true;
    video.src = video.dataset.src;
    video.load();
  }
  function reflect() {
    const playing = !video.paused && !video.ended;
    playButton.dataset.playing = String(playing);
    playButton.setAttribute('aria-label', playing ? 'Pause gameplay' : video.ended ? 'Replay gameplay' : 'Play gameplay');
    $('mobile-play-label').textContent = playing ? 'Pause' : video.ended ? 'Replay' : 'Play';
    soundButton.setAttribute('aria-pressed', String(!video.muted));
    soundButton.setAttribute('aria-label', video.muted ? 'Unmute gameplay' : 'Mute gameplay');
    $('mobile-sound-label').textContent = video.muted ? 'Unmute' : 'Mute';
  }
  function pause() {
    playRequest++;
    video.pause();
    reflect();
  }
  async function play() {
    if (!active() || document.hidden) return;
    hydrate();
    const request = ++playRequest;
    if (video.ended) video.currentTime = 0;
    filmStatus.textContent = '';
    try {
      await video.play();
      if (request !== playRequest) return;
      reflect();
    } catch (error) {
      if (request !== playRequest || error.name === 'AbortError') return;
      userPaused = true;
      filmStatus.textContent = 'Press play to watch the gameplay.';
      reflect();
    }
  }
  function sync() {
    if (!active() || document.hidden || !visible || userPaused) { pause(); return; }
    if (video.paused) void play();
  }
  playButton.addEventListener('click', () => {
    if (!video.paused && !video.ended) { userPaused = true; pause(); }
    else { userPaused = false; void play(); }
  });
  soundButton.addEventListener('click', () => {
    video.muted = !video.muted;
    // Silent footage can repeat. An audible excerpt ends without cutting the song
    // back to its beginning; the play control explicitly replays it.
    video.loop = video.muted;
    // The first request for sound starts the opening from the beginning.
    if (!video.muted && !heardOpening) {
      heardOpening = true;
      if (hydrated) video.currentTime = 0;
    }
    reflect();
    if (!video.muted) { userPaused = false; void play(); }
  });
  ['play', 'pause', 'volumechange', 'loadeddata'].forEach(name => video.addEventListener(name, reflect));
  video.addEventListener('play', followVideo);
  video.addEventListener('pause', stopFollowing);
  video.addEventListener('seeked', () => { drawKeyboard(video.currentTime); followVideo(); });
  video.addEventListener('loadeddata', () => drawKeyboard(video.currentTime));
  video.addEventListener('ended', () => { userPaused = true; reflect(); });
  video.addEventListener('error', () => {
    userPaused = true;
    filmStatus.textContent = 'The clip couldn’t load. You can still open the full game on your computer.';
    reflect();
  });
  document.addEventListener('visibilitychange', () => {
    if (document.hidden && !video.muted) userPaused = true;
    sync();
  });
  pointer.addEventListener?.('change', sync);
  motion.addEventListener?.('change', () => { if (motion.matches) userPaused = true; sync(); });
  connection?.addEventListener?.('change', () => { if (connection.saveData) userPaused = true; sync(); });
  if ('IntersectionObserver' in window) {
    new IntersectionObserver(entries => {
      visible = entries[0].isIntersecting;
      // Returning to the page never restarts audible music without a gesture.
      if (!visible && !video.muted) userPaused = true;
      sync();
    }, {threshold: 0}).observe(video);
  } else { visible = true; sync(); }
  reflect();
  drawKeyboard(0);

  const shareData = {
    title: 'Keyboard Warrior',
    url: 'https://keyboardwarrior.app/'
  };
  let sharing = false;
  $('mobile-share').addEventListener('click', async () => {
    if (sharing) return;
    sharing = true;
    const status = $('mobile-share-status');
    status.textContent = '';
    try {
      if (typeof navigator.share === 'function') {
        // Call directly during the tap, before any await can consume the
        // user activation Safari needs to open its native share sheet.
        try { await navigator.share(shareData); return; }
        catch (error) {
          if (error.name === 'AbortError') return;
        }
        // A failed native share can still copy the link; cancellation is final.
      }
      if (navigator.clipboard?.writeText) {
        try {
          await navigator.clipboard.writeText(shareData.url);
          $('mobile-share-fallback').hidden = true;
          status.textContent = 'Link copied. Open it on your computer.';
          return;
        } catch { /* Plain HTTP on local Wi-Fi uses the selectable link below. */ }
      }
      $('mobile-share-fallback').hidden = false;
      $('mobile-url').focus();
      $('mobile-url').select();
      status.textContent = 'Copy the link below.';
    } finally { sharing = false; }
  });
})();
