// Web audio backend for the browser demo: a ScriptProcessorNode pulls the
// game's own Rust mixer (audio::Mixer, exported as kw_render) for every
// buffer, so the browser plays the exact same mix the native cpal callback
// produces — and the mixer's frame counter stays the game clock.
//
// Registered as a miniquad plugin so `kw_audio_start` exists as a wasm
// import before the module is instantiated.
"use strict";
(function () {
    let ctx = null;
    let node = null;

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

        // Autoplay policy: a context created before any user gesture starts
        // suspended; resume it on the first key press or click.
        const resume = function () {
            if (ctx.state === "suspended") ctx.resume();
            window.removeEventListener("keydown", resume);
            window.removeEventListener("pointerdown", resume);
            window.removeEventListener("touchstart", resume);
        };
        window.addEventListener("keydown", resume);
        window.addEventListener("pointerdown", resume);
        window.addEventListener("touchstart", resume);

        return ctx.sampleRate;
    }

    miniquad_add_plugin({
        register_plugin: function (importObject) {
            importObject.env.kw_audio_start = kw_audio_start;
        },
        version: 1,
        name: "kw_audio",
    });
})();
