// Browser-side support for the demo: its display-paced frame clock, followed
// by the Web Audio backend. A ScriptProcessorNode pulls the game's own Rust
// mixer (audio::Mixer, exported as kw_render) for every buffer, so the browser
// plays the exact same mix the native cpal callback produces — and the mixer's
// frame counter stays the game clock.
//
// Registered as a miniquad plugin so `kw_audio_start` exists as a wasm
// import before the module is instantiated.
"use strict";
(function () {
    // One durable JSON string under one localStorage key, exposed to Rust as
    // the three imports a wasm string round-trip needs: ask the length, fill a
    // buffer Rust has sized to it, and write one back.
    //
    // The demo keeps two of these — the settings blob and the score table.
    // They're separate keys rather than one because they're written at
    // completely different rates (a settings change is rare, a score lands at
    // the end of every run) and because a parse failure in one must not cost
    // the player the other.
    function makeStore(key) {
        // Survives for the page session when durable storage is unavailable.
        let fallback = null;
        // The encoded bytes between a `load_len` and the `load` that takes them.
        let pending = null;

        function read() {
            try {
                const stored = window.localStorage.getItem(key);
                if (stored !== null) fallback = stored;
            } catch (_) {
                // Private browsing and embedded contexts can expose
                // localStorage but throw on access. The in-memory copy still
                // preserves everything for the rest of this page session.
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
                    // Keep the session fallback above when storage is denied.
                    return 0;
                }
            },
        };
    }

    const settingsStore = makeStore("keyboardwarrior.settings.v1");
    const scoresStore = makeStore("keyboardwarrior.scores.v1");

    // Chrome's last Mojave build has a long-standing high-DPI WebGL
    // presentation bug: the canvas visibly judders even while rAF and Chrome's
    // own frame counters remain at 60 Hz. On the affected ANGLE/OpenGL path,
    // a bare-canvas reproduction becomes smooth when it draws its background
    // instead of issuing glClear. Detect that narrow path here; current Mac
    // Chrome uses ANGLE/Metal and other browsers retain their normal clear.
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
            // Privacy settings may hide the unmasked renderer.
        }
        if (/\bMetal\b/i.test(renderer)) return false;
        if (/\bANGLE\b/i.test(renderer) && /\bOpenGL\b/i.test(renderer)) return true;
        // Chrome 116 is the final Chrome available on Mojave and uses this
        // legacy path. Only use the version fallback when no renderer was
        // exposed; a known non-OpenGL backend must not be opted in.
        return !renderer && Number(version[1]) <= 116;
    }

    // The game paints every pixel, so its WebGL surface is opaque even when it
    // is embedded in a larger page. Miniquad asks for the browser defaults,
    // which include alpha; Chrome must then blend each new canvas frame with
    // everything underneath it. That distinction is invisible on the plain
    // full-window demo, but expensive and scheduling-sensitive on the personal
    // site, where the canvas sits over gradients and a masked paper texture.
    // The browser default also enables multisample antialiasing, while the
    // native miniquad configuration uses one sample. Match that native path
    // and avoid resolving a full-size multisampled Retina framebuffer every
    // frame. Depth behavior remains untouched.
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

    // macroquad 0.4.16/miniquad 0.4.11 unconditionally begins every frame
    // with one color-only clear before the app draws. Suppress exactly that
    // first 0x4000 clear on the affected browser, then let every subsequent
    // clear through. This preserves future render-target/depth clears and
    // fails safely if the dependency ever changes the first clear's mask.
    const runGlClear = importObject.env.glClear;
    let suppressNextColorClear = false;
    importObject.env.glClear = function (mask) {
        if (suppressNextColorClear) {
            suppressNextColorClear = false;
            if (mask === 0x4000) return;
        }
        return runGlClear(mask);
    };

    // Miniquad throws away requestAnimationFrame's timestamp and implements
    // its Rust-side clock with Date.now(). Chrome can deliver callbacks at
    // uneven points inside otherwise evenly spaced display frames, especially
    // on older Intel Macs; sampling the callback's arrival time then bakes the
    // browser's scheduling noise into every animation position.
    //
    // Keep miniquad's clock monotonic, and while a frame is being drawn pin it
    // to the animation timestamp Chrome supplied for that frame. This fixes
    // the clock at the boundary rather than special-casing the song timeline:
    // get_time(), get_frame_time(), UI motion, and gameplay all agree on the
    // same display-paced instant. Code running outside rAF continues to see
    // the live performance clock.
    const clockEpoch = Number.isFinite(performance.timeOrigin)
        ? performance.timeOrigin
        : Date.now() - performance.now();
    let frameTime = null;
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

    // Game clock support. The mixer's frame counter counts frames *rendered*,
    // but a ScriptProcessorNode renders ahead of the speaker and, because its
    // callback runs on the main thread, in bursts — and a callback that misses
    // its deadline makes the node emit a period of silence, which pushes every
    // later frame further into the future. So rendered frames run ahead of
    // heard frames by an amount that jitters and grows.
    //
    // These two track the mapping. `rendered` is the same count the Rust side
    // keeps; `anchorFrame` is heard at `anchorTime` on the context clock, taken
    // from the callback's own playbackTime. From that, kw_audio_lag reports how
    // many rendered frames have not reached the speaker yet, and Rust subtracts
    // it to get a clock that follows the music instead of the renderer.
    let rendered = 0;
    let anchorFrame = 0;
    let anchorTime = 0;

    // Whether the game wants sound right now. False only across a blocking
    // decode (see kw_audio_suspend). Every path that could start the context
    // goes through resumeIfWanted, so a keypress landing mid-decode can't
    // restart it behind the decode's back.
    let wantRunning = true;

    // Ask the browser to start (or restart) the context, if the game wants it.
    //
    // The condition is "not running", never "is suspended". A context created
    // before any user gesture doesn't report "suspended" on WebKit — it reports
    // "interrupted", a state no other engine has. Every resume here used to
    // guard on "suspended", so on Safari not one of them ever fired: the
    // context sat interrupted, the ScriptProcessorNode was never pulled, and
    // because the game clock IS the mixer's frame counter (audio.rs), silence
    // read as a stopped clock — the highway froze on READY and stayed there.
    //
    // "Not running" is true in every state that needs a resume, in every
    // browser, including the interruptions WebKit raises long after startup
    // (output device change, display sleep) that would otherwise kill the
    // audio mid-song.
    function resumeIfWanted() {
        if (!ctx || !wantRunning || ctx.state === "running") return;
        // A refusal is normal — resume() outside a user gesture is denied, and
        // the next keypress calls this again — so the rejection is swallowed
        // rather than surfacing as an unhandled promise.
        const p = ctx.resume();
        if (p && p.catch) p.catch(function () {});
    }

    function kw_audio_start() {
        ctx = new (window.AudioContext || window.webkitAudioContext)();
        // 2048-frame pulls: ~43 ms at 48 kHz. Small enough that the game
        // clock stays smooth, large enough that a main-thread callback
        // doesn't underrun every time a frame runs long.
        node = ctx.createScriptProcessor(2048, 0, 2);
        node.onaudioprocess = function (e) {
            // Pull counter, visible from the console for sync debugging
            window.__kw_pulls = (window.__kw_pulls || 0) + 1;
            const out = e.outputBuffer;
            const n = out.length;
            // This buffer's first frame is heard at playbackTime. A buffer
            // being filled now can't already have played, so anything at or
            // behind the context clock is a browser that doesn't report it
            // (older WebKit says 0) — fall back to "one buffer from now",
            // which is what the spec's value amounts to.
            const pt =
                e.playbackTime > ctx.currentTime
                    ? e.playbackTime
                    : ctx.currentTime + n / ctx.sampleRate;
            anchorFrame = rendered;
            anchorTime = pt;
            rendered += n;
            const ptr = wasm_exports.kw_render(n);
            const mix = new Float32Array(wasm_memory.buffer, ptr, n * 2);
            const l = out.getChannelData(0);
            const r = out.getChannelData(1);
            for (let i = 0; i < n; i++) {
                l[i] = mix[i * 2];
                r[i] = mix[i * 2 + 1];
            }
        };
        node.connect(ctx.destination);

        // Autoplay policy: a context created before any user gesture doesn't
        // start; the first key press or click is what's allowed to start it.
        //
        // The listeners stay registered for the whole session rather than
        // removing themselves on the first event. A resume can be refused —
        // and on WebKit the state it has to climb out of ("interrupted") can
        // be re-entered at any time, when the output device changes or the
        // display sleeps. One-shot listeners meant the demo got exactly one
        // chance at sound and, if that chance failed, never got another.
        window.addEventListener("keydown", resumeIfWanted);
        window.addEventListener("pointerdown", resumeIfWanted);
        window.addEventListener("touchstart", resumeIfWanted);
        // An interruption that arrives while the tab has focus needs no gesture
        // to recover from, so don't make the player press a key to get the
        // music back.
        ctx.addEventListener("statechange", resumeIfWanted);

        return ctx.sampleRate;
    }

    // Frames rendered but not yet heard. The game clock is `rendered - lag`,
    // which advances with the speaker: a burst of catch-up renders doesn't move
    // it, and a dropout holds it still instead of teleporting it forward.
    // Clamped because the extrapolation is only trustworthy for about as long
    // as the buffering itself — a suspended context freezes both counts, so the
    // lag simply holds, and the clock with it.
    function kw_audio_lag() {
        if (!ctx || !node) return 0;
        const heard = anchorFrame + (ctx.currentTime - anchorTime) * ctx.sampleRate;
        const lag = Math.min(Math.max(rendered - heard, 0), 4 * node.bufferSize);
        window.__kw_lag = lag; // visible from the console for sync debugging
        return lag;
    }

    // The wasm decode runs on this same (main) thread and blocks the event
    // loop for hundreds of ms, so onaudioprocess can't fire and the pipeline
    // underruns into clicks. The game suspends the context across a decode:
    // suspend halts the rendering thread (its own thread, unblocked by the
    // stalled main thread), so the gap is clean silence instead.
    //
    // The pair tracks intent in `wantRunning` rather than reading ctx.state,
    // because state lags the call: suspend() and resume() are both async, and
    // a resume that read a state the pending suspend hadn't reached yet would
    // decline to fire and leave the context parked for good — a decode that
    // silenced the rest of the session.
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

    // Rust uses the same decision to replace its own background clear with an
    // opaque full-screen quad. Together the two sides produce zero glClear
    // calls; changing only one side would leave the reproducer's trigger.
    function kw_webgl_avoid_default_clear() {
        return avoidDefaultClear ? 1 : 0;
    }

    // The demo's one non-audio hook: the menu's "download to expand library"
    // row opens the project page. It rides in this file rather than a script
    // of its own because the portfolio site serves its own index.html — a new
    // <script> tag there is outside this repo, and a missing import doesn't
    // degrade, it fails the whole wasm instantiation.
    //
    // The URL comes from Rust (web::DOWNLOAD_URL) as a pointer into the wasm
    // heap, so the address the menu prints on screen and the one opened here
    // can't drift apart.
    function kw_open_url(ptr, len) {
        const url = new TextDecoder().decode(new Uint8Array(wasm_memory.buffer, ptr, len));
        // The keypress that got here is a frame or two old, so the browser's
        // transient user activation normally still stands and a tab opens.
        // Blockers that disagree hand back null; navigating this tab is always
        // allowed and beats the key doing nothing at all.
        //
        // Deliberately no "noopener" in the feature string: with it, open()
        // returns null on SUCCESS as well as on failure, and the fallback below
        // then fires every time — sending the demo tab to the same page it just
        // opened in a new one. Severing .opener afterwards does the same job.
        const tab = window.open(url, "_blank");
        if (tab) tab.opener = null;
        else window.location.href = url;
    }

    miniquad_add_plugin({
        register_plugin: function (importObject) {
            importObject.env.kw_audio_start = kw_audio_start;
            importObject.env.kw_audio_lag = kw_audio_lag;
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
