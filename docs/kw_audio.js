// Browser clocks, storage, and audio imports for miniquad.
"use strict";
(function () {
    // Separate settings and score stores so a corrupt save cannot affect both.
    function makeStore(key) {
        // Session fallback when localStorage is unavailable.
        let fallback = null;
        // Encoded bytes shared by load_len() and load().
        let pending = null;

        function read() {
            try {
                const stored = window.localStorage.getItem(key);
                if (stored !== null) fallback = stored;
            } catch (_) {
                // Storage access can fail in private or embedded contexts.
            }
            return fallback;
        }

        return {
            load_len() {
                const json = read();
                pending = json === null ? null : new TextEncoder().encode(json);
                return pending === null ? 0 : pending.length;
            },
            load(ptr, len) {
                if (pending === null) return;
                const out = new Uint8Array(wasm_memory.buffer, ptr, len);
                out.set(pending.subarray(0, len));
                pending = null;
            },
            save(ptr, len) {
                const bytes = new Uint8Array(wasm_memory.buffer, ptr, len);
                fallback = new TextDecoder().decode(bytes);
                try {
                    window.localStorage.setItem(key, fallback);
                    return 1;
                } catch (_) {
                    return 0;
                }
            },
        };
    }

    const settingsStore = makeStore("keyboardwarrior.settings.v1");
    const scoresStore = makeStore("keyboardwarrior.scores.v1");

    // Work around frame judder on legacy Mac Chromium with ANGLE/OpenGL.
    let avoidDefaultClear = false;
    function legacyMacChromiumOpenGL(context) {
        const ua = navigator.userAgent || "";
        const version = ua.match(/(?:Chrome|Chromium)\/(\d+)/);
        if (
            navigator.vendor !== "Google Inc." ||
            !/Macintosh/.test(ua) ||
            !version
        ) {
            return false;
        }
        let renderer = "";
        try {
            const info = context.getExtension("WEBGL_debug_renderer_info");
            if (info) {
                renderer = String(
                    context.getParameter(info.UNMASKED_RENDERER_WEBGL) || ""
                );
            }
        } catch (_) {
            // Privacy settings may hide the renderer.
        }
        if (/\bMetal\b/i.test(renderer)) return false;
        if (/\bANGLE\b/i.test(renderer) && /\bOpenGL\b/i.test(renderer)) return true;
        // Use the Mojave version fallback only when the renderer is unknown.
        return !renderer && Number(version[1]) <= 116;
    }

    // Match the native opaque, single-sample framebuffer.
    const gameCanvas = document.querySelector("#glcanvas");
    if (gameCanvas) {
        const getContext = gameCanvas.getContext.bind(gameCanvas);
        gameCanvas.getContext = function (kind, attrs) {
            const isWebGL =
                kind === "webgl" || kind === "webgl2" || kind === "experimental-webgl";
            if (isWebGL) {
                attrs = Object.assign({}, attrs, { alpha: false, antialias: false });
            }
            const context = getContext(kind, attrs);
            if (isWebGL && context) {
                avoidDefaultClear = legacyMacChromiumOpenGL(context);
            }
            return context;
        };
    }

    // Skip only miniquad's initial color clear on affected browsers.
    // Preserve subsequent clears and other masks.
    const runGlClear = importObject.env.glClear;
    let suppressNextColorClear = false;
    importObject.env.glClear = function (mask) {
        if (suppressNextColorClear) {
            suppressNextColorClear = false;
            if (mask === 0x4000) return;
        }
        return runGlClear(mask);
    };

    // Use the display timestamp during rAF and the live clock between frames.
    const clockEpoch = Number.isFinite(performance.timeOrigin)
        ? performance.timeOrigin
        : Date.now() - performance.now();
    importObject.env.kw_input_clock = () => performance.now() / 1000;
    let frameTime = null;
    importObject.env.kw_input_frame_clock = () => (frameTime === null ? performance.now() : frameTime) / 1000;
    const runAnimationFrame = animation;
    animation = function (timestamp) {
        frameTime = timestamp;
        suppressNextColorClear = avoidDefaultClear;
        try {
            runAnimationFrame(timestamp);
        } finally {
            suppressNextColorClear = false;
            frameTime = null;
        }
    };
    importObject.env.now = function () {
        const now = frameTime === null ? performance.now() : frameTime;
        return (clockEpoch + now) / 1000;
    };

    let ctx = null;
    let node = null;

    // Map rendered frames to their playback time to measure queued audio.
    let rendered = 0;
    let anchorFrame = 0;
    let anchorTime = 0;

    // Track intent separately from async AudioContext state changes.
    let wantRunning = true;

    let hitSounds = [];

    // Prepare feedback samples once to keep decoding off the keydown path.
    function prepareHitSounds() {
        // The PCM bridge is absent when feedback sounds are disabled.
        if (typeof wasm_exports.kw_hit_pcm_rate !== "function") {
            hitSounds = [];
            return;
        }
        const sampleRate = wasm_exports.kw_hit_pcm_rate();
        const count = typeof wasm_exports.kw_hit_pcm_count === "function" ? wasm_exports.kw_hit_pcm_count() : 3;
        hitSounds = Array.from({length: count}, function (_, kind) {
            const ptr = wasm_exports.kw_hit_pcm_ptr(kind);
            const frames = wasm_exports.kw_hit_pcm_frames(kind);
            if (!ptr || !frames || !sampleRate) return null;
            const source = new DataView(wasm_memory.buffer, ptr, frames * 4);
            const buffer = ctx.createBuffer(2, frames, sampleRate);
            const left = buffer.getChannelData(0);
            const right = buffer.getChannelData(1);
            for (let i = 0; i < frames; i++) {
                left[i] = source.getInt16(i * 4, true) / 32768;
                right[i] = source.getInt16(i * 4 + 2, true) / 32768;
            }
            return buffer;
        });
    }

    // Resume from suspended or WebKit's interrupted state.
    function resumeIfWanted() {
        if (!ctx || !wantRunning || ctx.state === "running") return;
        // Autoplay may reject this; the next gesture retries.
        const p = ctx.resume();
        if (p && p.catch) p.catch(function () {});
    }

    function kw_audio_start() {
        ctx = new (window.AudioContext || window.webkitAudioContext)();
        prepareHitSounds();
        // 2048 frames balance latency and main-thread underruns (~43 ms at 48 kHz).
        node = ctx.createScriptProcessor(2048, 0, 2);
        node.onaudioprocess = function (e) {
            window.__kw_pulls = (window.__kw_pulls || 0) + 1;
            const out = e.outputBuffer;
            const n = out.length;
            // Older WebKit reports playbackTime as zero; estimate one buffer ahead.
            const pt =
                e.playbackTime > ctx.currentTime
                    ? e.playbackTime
                    : ctx.currentTime + n / ctx.sampleRate;
            anchorFrame = rendered;
            anchorTime = pt;
            rendered += n;
            const ptr = wasm_exports.kw_render(n);
            if (!ptr) return;
            const mix = new Float32Array(wasm_memory.buffer, ptr, n * 2);
            const l = out.getChannelData(0);
            const r = out.getChannelData(1);
            for (let i = 0; i < n; i++) {
                l[i] = mix[i * 2];
                r[i] = mix[i * 2 + 1];
            }
        };
        node.connect(ctx.destination);

        // Keep gesture listeners active to recover from later interruptions.
        window.addEventListener("keydown", resumeIfWanted);
        window.addEventListener("pointerdown", resumeIfWanted);
        window.addEventListener("touchstart", resumeIfWanted);
        ctx.addEventListener("statechange", resumeIfWanted);

        return ctx.sampleRate;
    }

    // Frames queued ahead of the speaker, clamped to four buffers.
    function kw_audio_lag() {
        if (!ctx || !node) return 0;
        const heard = anchorFrame + (ctx.currentTime - anchorTime) * ctx.sampleRate;
        const lag = Math.min(Math.max(rendered - heard, 0), 4 * node.bufferSize);
        window.__kw_lag = lag; // visible from the console for sync debugging
        return lag;
    }

    // Play key feedback directly to bypass the 2048-frame song buffer.
    function kw_audio_hit(kind, volume) {
        if (!ctx || !Number.isFinite(volume) || volume <= 0) return;
        const buffer = hitSounds[kind];
        if (!buffer) return;
        const source = ctx.createBufferSource();
        const gain = ctx.createGain();
        source.buffer = buffer;
        gain.gain.value = Math.min(volume, 1);
        source.connect(gain);
        gain.connect(ctx.destination);
        source.start(ctx.currentTime);
    }

    // Suspend during blocking WASM decoding to prevent underrun clicks.
    // Use wantRunning because suspend/resume state changes are async.
    function kw_audio_suspend() {
        wantRunning = false;
        if (!ctx) return;
        const p = ctx.suspend();
        if (p && p.catch) p.catch(function () {});
    }
    function kw_audio_resume() {
        wantRunning = true;
        resumeIfWanted();
    }

    // Rust also replaces its background clear with an opaque quad.
    function kw_webgl_avoid_default_clear() {
        return avoidDefaultClear ? 1 : 0;
    }

    function kw_open_url(ptr, len) {
        const url = new TextDecoder().decode(new Uint8Array(wasm_memory.buffer, ptr, len));
        // Setting noopener in open() returns null even on success.
        // Clear opener afterwards so null still identifies a blocked popup.
        const tab = window.open(url, "_blank");
        if (tab) tab.opener = null;
        else window.location.href = url;
    }

    // Remove cancelled fetches before aborting to ignore late results.
    const songFetches = new Map();
    let nextSongFetch = 1;
    function kw_song_fetch_start(ptr, len) {
        const url = new TextDecoder().decode(new Uint8Array(wasm_memory.buffer, ptr, len));
        const id = nextSongFetch++;
        const task = { controller: new AbortController(), bytes: null, failed: false };
        songFetches.set(id, task);
        fetch(url, { signal: task.controller.signal }).then(response => {
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            return response.arrayBuffer();
        }).then(bytes => {
            if (songFetches.get(id) === task) task.bytes = new Uint8Array(bytes);
        }).catch(() => {
            if (songFetches.get(id) === task) task.failed = true;
        });
        return id;
    }
    function kw_song_fetch_size(id) {
        const task = songFetches.get(id);
        if (!task || task.failed) return -1;
        return task.bytes === null ? 0 : (task.bytes.length || -1);
    }
    function kw_song_fetch_read(id, ptr, len) {
        const task = songFetches.get(id);
        if (task && task.bytes) new Uint8Array(wasm_memory.buffer, ptr, len).set(task.bytes.subarray(0, len));
        songFetches.delete(id);
    }
    function kw_song_fetch_cancel(id) {
        const task = songFetches.get(id);
        songFetches.delete(id);
        if (task) task.controller.abort();
    }

    miniquad_add_plugin({
        register_plugin: function (importObject) {
            importObject.env.kw_song_fetch_start = kw_song_fetch_start;
            importObject.env.kw_song_fetch_size = kw_song_fetch_size;
            importObject.env.kw_song_fetch_read = kw_song_fetch_read;
            importObject.env.kw_song_fetch_cancel = kw_song_fetch_cancel;
            importObject.env.kw_audio_start = kw_audio_start;
            importObject.env.kw_audio_lag = kw_audio_lag;
            importObject.env.kw_audio_hit = kw_audio_hit;
            importObject.env.kw_audio_suspend = kw_audio_suspend;
            importObject.env.kw_audio_resume = kw_audio_resume;
            importObject.env.kw_webgl_avoid_default_clear = kw_webgl_avoid_default_clear;
            importObject.env.kw_open_url = kw_open_url;
            importObject.env.kw_settings_load_len = settingsStore.load_len;
            importObject.env.kw_settings_load = settingsStore.load;
            importObject.env.kw_settings_save = settingsStore.save;
            importObject.env.kw_scores_load_len = scoresStore.load_len;
            importObject.env.kw_scores_load = scoresStore.load;
            importObject.env.kw_scores_save = scoresStore.save;
        },
        version: 1,
        name: "kw_audio",
    });
})();
