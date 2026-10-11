/* ytm-rs site. No framework, no build step. */
(() => {
  "use strict";

  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduced) document.documentElement.classList.add("no-motion");

  /* ---------------- smooth scroll ---------------- */
  let lenis = null;
  if (!reduced && window.Lenis) {
    lenis = new Lenis({ lerp: 0.11, wheelMultiplier: 1 });
    const raf = (t) => { lenis.raf(t); requestAnimationFrame(raf); };
    requestAnimationFrame(raf);
  }
  $$('a[href^="#"]').forEach((a) => {
    a.addEventListener("click", (e) => {
      const id = a.getAttribute("href");
      const el = id.length > 1 ? $(id) : null;
      if (!el && id !== "#top") return;
      e.preventDefault();
      if (lenis) lenis.scrollTo(id === "#top" ? 0 : el, { offset: -70, duration: 1.4 });
      else (el || document.body).scrollIntoView({ behavior: reduced ? "auto" : "smooth" });
      history.replaceState(null, "", id);
    });
  });

  /* ---------------- scroll-driven bits (one handler) ---------------- */
  const nav = $("#nav");
  const scrollJobs = [];
  const onScroll = () => {
    nav.classList.toggle("is-scrolled", scrollY > 24);
    for (const job of scrollJobs) job();
  };
  if (lenis) lenis.on("scroll", onScroll);
  else addEventListener("scroll", onScroll, { passive: true });
  addEventListener("resize", onScroll);

  // container-scroll-animation: the window tilts back and flattens as you scroll into it
  const frame = $("[data-scrollframe]");
  const device = $("[data-device]");
  if (frame && device && !reduced) {
    scrollJobs.push(() => {
      if (innerWidth <= 700) return;
      const r = frame.getBoundingClientRect();
      const p = clamp((innerHeight - r.top) / (innerHeight * 1.25), 0, 1);
      const e = 1 - Math.pow(1 - p, 2);
      device.style.setProperty("--rot", `${(22 * (1 - e)).toFixed(2)}deg`);
      device.style.setProperty("--scale", (1.04 - 0.04 * e).toFixed(4));
    });
  }

  // text-reveal: words light up as the sticky block scrolls past
  const reveal = $("[data-reveal]");
  const revealText = $("[data-reveal-text]");
  if (reveal && revealText) {
    const hl = new Set(["itself,", "directly.", "sleeps"]);
    revealText.innerHTML = revealText.textContent.trim().split(/\s+/)
      .map((w) => `<span class="w${hl.has(w) ? " hl" : ""}">${w}</span>`).join(" ");
    const words = $$(".w", revealText);
    scrollJobs.push(() => {
      if (reduced) return;
      const r = reveal.getBoundingClientRect();
      const p = clamp(-r.top / (r.height - innerHeight), 0, 1);
      const n = Math.round(p * 1.15 * words.length);
      words.forEach((w, i) => w.classList.toggle("on", i < n));
    });
  }
  onScroll();

  /* ---------------- hero headline: karaoke sweep ---------------- */
  const kws = $$(".hero__title .kw");
  if (reduced) kws.forEach((w) => w.style.setProperty("--p", 1.2));
  else {
    const per = 360, start = performance.now() + 450;
    const step = (now) => {
      let done = true;
      kws.forEach((w, i) => {
        const p = clamp((now - start - i * per) / (per * 1.15), 0, 1.2);
        if (p < 1.2) done = false;
        w.style.setProperty("--p", p.toFixed(3));
      });
      if (!done) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  }

  /* ---------------- entrance rise ---------------- */
  const riseEls = $$(".section__head, .card, .note, .gallery__item, .dl, .faq__list, .themes__copy, .player, .themepick, .beams, .showcase");
  if (!reduced && "IntersectionObserver" in window) {
    riseEls.forEach((el) => el.classList.add("rise"));
    const io = new IntersectionObserver((es) => es.forEach((e) => {
      if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); }
    }), { rootMargin: "0px 0px -8% 0px" });
    riseEls.forEach((el) => io.observe(el));
  }

  /* ---------------- number ticker (spring, like framer's useSpring) ---------------- */
  const tickers = $$("[data-tick]");
  const runTicker = (el) => {
    const to = +el.dataset.tick, from = el.dataset.tick === "0" ? 60 : 0, prefix = el.dataset.prefix || "";
    if (reduced) { el.textContent = prefix + to; return; }
    let x = from, v = 0, last = performance.now();
    const k = 100, c = 60 / 3; // stiffness, damping (scaled for a snappier settle)
    const tick = (now) => {
      const dt = Math.min(0.032, (now - last) / 1000); last = now;
      const a = -k * (x - to) - c * v; v += a * dt; x += v * dt;
      el.textContent = prefix + Math.max(0, Math.round(x));
      if (Math.abs(x - to) > 0.02 || Math.abs(v) > 0.02) requestAnimationFrame(tick);
      else el.textContent = prefix + to;
    };
    requestAnimationFrame(tick);
  };
  if ("IntersectionObserver" in window) {
    const io = new IntersectionObserver((es) => es.forEach((e) => {
      if (e.isIntersecting) { runTicker(e.target); io.unobserve(e.target); }
    }), { threshold: 0.6 });
    tickers.forEach((t) => io.observe(t));
  } else tickers.forEach(runTicker);

  /* ---------------- generated cover art ---------------- */
  function heroArt() {
    const c = document.createElement("canvas"); c.width = c.height = 512;
    const g = c.getContext("2d");
    g.fillStyle = "#2a0407"; g.fillRect(0, 0, 512, 512);
    const blob = (x, y, r, col) => {
      const gr = g.createRadialGradient(x * 512, y * 512, 0, x * 512, y * 512, r * 512);
      gr.addColorStop(0, col); gr.addColorStop(1, "rgba(0,0,0,0)");
      g.fillStyle = gr; g.fillRect(0, 0, 512, 512);
    };
    blob(0.28, 0.32, 0.5, "#ff1a2b");
    blob(0.78, 0.26, 0.38, "#ff8a1f");
    blob(0.62, 0.82, 0.42, "#ff3d7f");
    blob(0.12, 0.86, 0.36, "#4a0c33");
    blob(0.52, 0.5, 0.16, "#ffd2a6");
    blob(0.9, 0.7, 0.3, "#b3122a");
    return c;
  }

  // the demo song's cover: a night window, drawn once and reused as the warp texture
  function drawDemoCover(c) {
    const g = c.getContext("2d"), S = c.width;
    const sky = g.createLinearGradient(0, 0, 0, S);
    sky.addColorStop(0, "#0d1440"); sky.addColorStop(0.55, "#5a2a7a"); sky.addColorStop(0.85, "#ff6a5c"); sky.addColorStop(1, "#ffb36b");
    g.fillStyle = sky; g.fillRect(0, 0, S, S);
    // stars
    let seed = 7; const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
    for (let i = 0; i < 90; i++) {
      g.fillStyle = `rgba(255,255,255,${0.25 + rnd() * 0.6})`;
      const r = rnd() * 1.6 + 0.4; g.beginPath(); g.arc(rnd() * S, rnd() * S * 0.55, r, 0, 7); g.fill();
    }
    // moon with halo
    const halo = g.createRadialGradient(S * 0.66, S * 0.34, 0, S * 0.66, S * 0.34, S * 0.3);
    halo.addColorStop(0, "rgba(255,235,210,.55)"); halo.addColorStop(1, "rgba(255,235,210,0)");
    g.fillStyle = halo; g.fillRect(0, 0, S, S);
    g.fillStyle = "#fff1dc"; g.beginPath(); g.arc(S * 0.66, S * 0.34, S * 0.085, 0, 7); g.fill();
    // city line
    g.fillStyle = "#1a0c22";
    let x = 0; seed = 3;
    while (x < S) { const w = 18 + rnd() * 40, h = S * (0.1 + rnd() * 0.16); g.fillRect(x, S - h, w, h); x += w + 2; }
    g.fillStyle = "rgba(255,200,120,.85)";
    for (let i = 0; i < 40; i++) g.fillRect(rnd() * S, S - rnd() * S * 0.2, 3, 4);
    // window frame
    g.strokeStyle = "#12070f"; g.lineWidth = S * 0.035;
    g.strokeRect(S * 0.08, S * 0.08, S * 0.84, S * 0.84);
    g.beginPath(); g.moveTo(S / 2, S * 0.08); g.lineTo(S / 2, S * 0.92); g.moveTo(S * 0.08, S * 0.5); g.lineTo(S * 0.92, S * 0.5); g.stroke();
    g.fillStyle = "rgba(255,255,255,.07)"; g.fillRect(S * 0.1, S * 0.1, S * 0.38, S * 0.38);
    // title
    g.fillStyle = "rgba(255,255,255,.92)"; g.font = `700 ${S * 0.05}px "Noto Sans JP", "Yu Gothic UI", sans-serif`;
    g.fillText("窓の外", S * 0.12, S * 0.17);
    return c;
  }

  /* ---------------- WebGL warp (Kawarp-style) ---------------- */
  const VS = "attribute vec2 p;void main(){gl_Position=vec4(p,0.,1.);}";
  const FS = `precision mediump float;
uniform sampler2D tex;uniform vec2 res;uniform float t,sat,still;
vec2 rot(vec2 v,float a){float c=cos(a),s=sin(a);return mat2(c,-s,s,c)*v;}
vec3 layer(vec2 uv,float sp,float sc,float ph){
  vec2 q=rot(uv-.5,t*sp+ph)*sc;
  q+=.09*vec2(sin(q.y*4.+t*.7+ph),cos(q.x*3.5-t*.6+ph));
  return texture2D(tex,q+.5,4.5).rgb;}
void main(){
  vec2 uv=gl_FragCoord.xy/res;float asp=res.x/res.y;
  vec2 u=vec2((uv.x-.5)*max(asp,1.)+.5,(uv.y-.5)*max(1./asp,1.)+.5);
  vec3 c;
  if(still>.5){c=texture2D(tex,(u-.5)*.86+.5+vec2(sin(t*.1),cos(t*.13))*.02,3.).rgb;}
  else{c=layer(u,.05,1.1,0.)*.45+layer(u,-.035,.75,2.1)*.35+layer(u,.022,1.6,4.2)*.2;}
  float l=dot(c,vec3(.299,.587,.114));c=mix(vec3(l),c,sat);
  float v=smoothstep(1.25,.25,length(uv-.5));c*=mix(.55,1.,v);
  gl_FragColor=vec4(c,1.);}`;

  function makeWarp(canvas, source, opts = {}) {
    const gl = canvas.getContext("webgl", { antialias: false, alpha: false, preserveDrawingBuffer: false, powerPreference: "low-power" });
    if (!gl) { canvas.style.background = "radial-gradient(circle at 30% 30%, #ff2433, #3a0610 70%)"; canvas.classList.add("is-ready"); return null; }
    const sh = (type, src) => { const s = gl.createShader(type); gl.shaderSource(s, src); gl.compileShader(s); return s; };
    const pr = gl.createProgram();
    gl.attachShader(pr, sh(gl.VERTEX_SHADER, VS)); gl.attachShader(pr, sh(gl.FRAGMENT_SHADER, FS)); gl.linkProgram(pr);
    if (!gl.getProgramParameter(pr, gl.LINK_STATUS)) return null;
    gl.useProgram(pr);
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(pr, "p"); gl.enableVertexAttribArray(loc); gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
    const tex = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.MIRRORED_REPEAT);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.MIRRORED_REPEAT);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    const upload = (src) => { gl.bindTexture(gl.TEXTURE_2D, tex); gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, src); gl.generateMipmap(gl.TEXTURE_2D); };
    upload(source);
    const U = (n) => gl.getUniformLocation(pr, n);
    const uRes = U("res"), uT = U("t"), uSat = U("sat"), uStill = U("still");
    const state = { sat: opts.sat ?? 1.3, still: 0, visible: true, t0: performance.now() - 20000, raf: 0 };
    const scale = opts.scale ?? 0.18; // render small, the GPU's bilinear upscale is the blur
    const resize = () => {
      const r = canvas.getBoundingClientRect();
      canvas.width = Math.max(32, Math.round(r.width * scale));
      canvas.height = Math.max(32, Math.round(r.height * scale));
      gl.viewport(0, 0, canvas.width, canvas.height);
      draw(performance.now());
    };
    const draw = (now) => {
      gl.uniform2f(uRes, canvas.width, canvas.height);
      gl.uniform1f(uT, (now - state.t0) / 1000 * (opts.speed ?? 1));
      gl.uniform1f(uSat, state.sat); gl.uniform1f(uStill, state.still);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    };
    let last = 0;
    const loop = (now) => {
      state.raf = 0;
      if (!state.visible || document.hidden) return;
      if (now - last > 1000 / 30) { draw(now); last = now; } // 30 fps is plenty for a slow warp, same budget as the app
      state.raf = requestAnimationFrame(loop);
    };
    const start = () => { if (!reduced && !state.raf) state.raf = requestAnimationFrame(loop); };
    new ResizeObserver(resize).observe(canvas);
    new IntersectionObserver((es) => { state.visible = es[0].isIntersecting; if (state.visible) start(); }).observe(canvas);
    document.addEventListener("visibilitychange", () => { if (!document.hidden) start(); });
    resize(); start();
    requestAnimationFrame(() => canvas.classList.add("is-ready"));
    return { set(o) { Object.assign(state, o); if (reduced) draw(performance.now()); }, upload };
  }

  const heroCanvas = $('[data-warp="hero"]');
  if (heroCanvas) makeWarp(heroCanvas, heroArt(), { sat: 1.35, scale: 0.14, speed: 1.4 });

  /* ---------------- lyrics demo ---------------- */
  const LINES = [
    { t: 1.6, jp: "窓の外は まだ夜のまま", ro: "mado no soto wa mada yoru no mama", tr: "Outside the window, it's still night" },
    { t: 5.8, jp: "君の声だけ 鳴り続けてる", ro: "kimi no koe dake naritsuzuketeru", tr: "Only your voice keeps on playing" },
    { t: 10.0, jp: "重たい画面は もういらない", ro: "omotai gamen wa mō iranai", tr: "I don't need the heavy screens anymore" },
    { t: 14.2, jp: "軽くなって 走り出そう", ro: "karuku natte hashiridasō", tr: "Let's get lighter and start running" },
    { t: 18.4, jp: "ひとつの音も 落とさずに", ro: "hitotsu no oto mo otosazu ni", tr: "Without dropping a single note" },
    { t: 22.6, jp: "朝が来るまで 歌っていて", ro: "asa ga kuru made utatte ite", tr: "Keep singing until the morning comes" },
  ];
  const DUR = 30;
  // numbers from assets/themes/presets.json in the app (alpha, falloff, past, blur, scope)
  const STYLES = {
    classic:   { ina: 0.38, fall: 0.03, min: 0.22, past: 0.38, blur: 0,   scope: "none", unsung: 0.42, sat: 1.25, dim: 0.36, still: 0, coverR: 14 },
    apple:     { ina: 0.2,  fall: 0.01, min: 0.1,  past: 0,    blur: 2,   scope: "up",   unsung: 0.45, sat: 2.0,  dim: 0.45, still: 0, coverR: 12 },
    glow:      { ina: 0.43, fall: 0.08, min: 0.145, past: 0.145, blur: 0.6, scope: "all", unsung: 0.45, sat: 1.35, dim: 0.52, still: 0, coverR: 18 },
    burn:      { ina: 1,    fall: 0,    min: 1,    past: 0,    blur: 0,   scope: "none", unsung: 1,    sat: 1.0,  dim: 0.75, still: 1, coverR: 28 },
    karaoke:   { ina: 0.22, fall: 0,    min: 0.22, past: 0.22, blur: 0,   scope: "none", unsung: 0.18, sat: 1.0,  dim: 0.4,  still: 1, coverR: 24 },
    spotlight: { ina: 0.3,  fall: 0.02, min: 0.12, past: 0.3,  blur: 2.5, scope: "past", unsung: 0.2,  sat: 1.25, dim: 0.45, still: 0, coverR: 14 },
  };

  const player = $("[data-player]");
  if (player) {
    const track = $("[data-track]", player);
    const box = $("[data-lyrics]", player);
    const playBtn = $("[data-play]", player);
    const fill = $("[data-fill]", player);
    const prog = $("[data-progress]", player);
    const timeEl = $("[data-time]", player);
    $("[data-dur]", player).textContent = "0:30";
    const cover = drawDemoCover($("[data-cover]", player));
    const warp = makeWarp($('[data-warp="player"]', player), cover, { sat: 1.25, scale: 0.12, speed: 1.6 });

    track.innerHTML = LINES.map((l, i) => `<button type="button" class="lyr" data-i="${i}" aria-label="Jump to line ${i + 1}: ${l.tr}"><span class="lyr__jp" lang="ja" data-text="${l.jp}"><span class="lyr__fill">${l.jp}</span></span><span class="lyr__ro">${l.ro}</span><span class="lyr__tr">${l.tr}</span></button>`).join("");
    const rows = $$(".lyr", track);
    player.classList.add("show-romaji", "show-trans");

    let style = STYLES.classic, t = 0, playing = !reduced, visible = false, lastNow = 0, active = -2;
    const fmt = (s) => `0:${String(Math.floor(s)).padStart(2, "0")}`;
    const activeAt = (s) => { let a = -1; LINES.forEach((l, i) => { if (s >= l.t) a = i; }); return a; };

    const layout = () => {
      const anchor = box.clientHeight * 0.42;
      const i = Math.max(0, active);
      const row = rows[i];
      const y = anchor - (row.offsetTop + row.offsetHeight / 2);
      track.style.transform = `translateY(${y.toFixed(1)}px)`;
    };
    const paintLines = () => {
      rows.forEach((row, i) => {
        const d = i - active;
        let a, blur = 0;
        if (d === 0) a = 1;
        else if (d < 0) a = style.past;
        else a = Math.max(style.ina - Math.min(d, 6) * style.fall, Math.min(style.min, style.ina));
        if (d !== 0 && style.blur) {
          const on = style.scope === "all" || (style.scope === "up" && d > 0) || (style.scope === "past" && d < 0);
          if (on) blur = style.blur * 1.15 * Math.min(Math.abs(d), 3);
        }
        row.style.setProperty("--a", a);
        row.style.setProperty("--blur", `${blur.toFixed(2)}px`);
        row.classList.toggle("is-active", d === 0);
        row.classList.toggle("is-past", d < 0);
      });
      player.style.setProperty("--unsung", `rgba(255,255,255,${style.unsung})`);
      layout();
    };
    const render = () => {
      const a = activeAt(t);
      if (a !== active) { active = a; paintLines(); }
      if (active >= 0) {
        const start = LINES[active].t, end = (LINES[active + 1]?.t ?? DUR - 3);
        rows[active].style.setProperty("--p", clamp((t - start) / ((end - start) * 0.82), 0, 1.1).toFixed(3));
      }
      fill.style.width = `${(t / DUR) * 100}%`;
      prog.setAttribute("aria-valuenow", Math.round((t / DUR) * 100));
      timeEl.textContent = fmt(t);
    };
    const loop = (now) => {
      const dt = lastNow ? (now - lastNow) / 1000 : 0; lastNow = now;
      if (playing) { t += Math.min(dt, 0.1); if (t >= DUR) t = 0; }
      render();
      if (visible && playing && !document.hidden) requestAnimationFrame(loop); else lastNow = 0;
    };
    const kick = () => { if (visible && playing) { lastNow = 0; requestAnimationFrame(loop); } };
    new IntersectionObserver((es) => { visible = es[0].isIntersecting; kick(); }, { threshold: 0.2 }).observe(player);
    document.addEventListener("visibilitychange", kick);

    const setPlaying = (p) => {
      playing = p; playBtn.classList.toggle("is-paused", !p);
      playBtn.setAttribute("aria-label", p ? "Pause" : "Play"); kick();
    };
    setPlaying(playing);
    playBtn.addEventListener("click", () => setPlaying(!playing));
    rows.forEach((row) => row.addEventListener("click", () => {
      t = LINES[+row.dataset.i].t + 0.01; render(); if (!playing) setPlaying(true);
    }));
    const seekTo = (clientX) => { const r = prog.getBoundingClientRect(); t = clamp((clientX - r.left) / r.width, 0, 0.999) * DUR; render(); };
    prog.addEventListener("pointerdown", (e) => { seekTo(e.clientX); prog.setPointerCapture(e.pointerId); });
    prog.addEventListener("pointermove", (e) => { if (prog.hasPointerCapture(e.pointerId)) seekTo(e.clientX); });
    prog.addEventListener("keydown", (e) => {
      if (e.key === "ArrowRight" || e.key === "ArrowLeft") { t = clamp(t + (e.key === "ArrowRight" ? 2 : -2), 0, DUR - 0.01); render(); e.preventDefault(); }
    });

    $$("[data-toggle]", player).forEach((b) => b.addEventListener("click", () => {
      const on = b.getAttribute("aria-pressed") !== "true";
      b.setAttribute("aria-pressed", on);
      player.classList.toggle(b.dataset.toggle === "romaji" ? "show-romaji" : "show-trans", on);
      layout();
    }));
    const picks = $$(".themepick [data-style]");
    picks.forEach((b) => b.addEventListener("click", () => {
      picks.forEach((x) => x.setAttribute("aria-checked", x === b));
      style = STYLES[b.dataset.style];
      player.dataset.theme = b.dataset.style;
      player.style.setProperty("--dim", style.dim);
      player.style.setProperty("--cover-r", `${style.coverR}px`);
      warp && warp.set({ sat: style.sat, still: style.still });
      paintLines();
    }));
    addEventListener("resize", layout);
    document.fonts?.ready.then(() => { drawDemoCover($("[data-cover]", player)); warp && warp.upload(cover); layout(); });
    render(); paintLines();
  }

  /* ---------------- glowing-effect on bento cards ---------------- */
  const cards = $$("[data-glow]");
  if (cards.length && matchMedia("(hover: hover)").matches && !reduced) {
    let mx = -1e4, my = -1e4, raf = 0, idle = 0;
    const tick = () => {
      raf = 0; let moving = false;
      for (const el of cards) {
        const r = el.getBoundingClientRect();
        const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
        const prox = 64;
        const inside = mx > r.left - prox && mx < r.right + prox && my > r.top - prox && my < r.bottom + prox;
        const dead = Math.hypot(mx - cx, my - cy) < 0.5 * Math.min(r.width, r.height) * 0.01;
        el.style.setProperty("--active", inside && !dead ? 1 : 0);
        if (!inside) continue;
        const cur = el._a ?? 0;
        const target = (180 * Math.atan2(my - cy, mx - cx)) / Math.PI + 90;
        const diff = ((target - cur + 540) % 360) - 180;
        const next = cur + diff * 0.14;
        el._a = next; el.style.setProperty("--start", next.toFixed(2));
        if (Math.abs(diff) > 0.3) moving = true;
      }
      if (moving || ++idle < 4) raf = requestAnimationFrame(tick);
    };
    const poke = () => { idle = 0; if (!raf) raf = requestAnimationFrame(tick); };
    document.addEventListener("pointermove", (e) => { mx = e.clientX; my = e.clientY; poke(); }, { passive: true });
    scrollJobs.push(poke);
  }

  /* ---------------- theme showcase tabs ---------------- */
  const tabs = $$("[data-shot]");
  const shots = $$("[data-shot-img]");
  let tabTimer = 0, tabIdx = 0, userPicked = false;
  const showShot = (i) => {
    tabIdx = i;
    tabs.forEach((t, j) => t.setAttribute("aria-selected", j === i));
    shots.forEach((s) => s.classList.toggle("is-on", s.dataset.shotImg === tabs[i].dataset.shot));
  };
  tabs.forEach((t, i) => t.addEventListener("click", () => { userPicked = true; clearInterval(tabTimer); showShot(i); }));
  const showcase = $("[data-showcase]");
  if (showcase && !reduced) {
    new IntersectionObserver((es) => {
      clearInterval(tabTimer);
      if (es[0].isIntersecting && !userPicked) tabTimer = setInterval(() => showShot((tabIdx + 1) % tabs.length), 3800);
    }, { threshold: 0.4 }).observe(showcase);
  }

  /* ---------------- animated beams ---------------- */
  const beams = $("[data-beams]");
  if (beams) {
    const svg = $("[data-beams-svg]", beams);
    const NS = "http://www.w3.org/2000/svg";
    const build = () => {
      $$("path", svg).forEach((p) => p.remove());
      if (innerWidth <= 760) return;
      const b = beams.getBoundingClientRect();
      const hub = $('[data-node="hub"]', beams).getBoundingClientRect();
      const pt = (r, side) => [side === "l" ? r.left - b.left : side === "r" ? r.right - b.left : r.left - b.left + r.width / 2, r.top - b.top + r.height / 2];
      const curve = ([x1, y1], [x2, y2]) => `M${x1},${y1} C${(x1 + x2) / 2},${y1} ${(x1 + x2) / 2},${y2} ${x2},${y2}`;
      const add = (d, i, color) => {
        const tr = document.createElementNS(NS, "path"); tr.setAttribute("d", d); tr.setAttribute("class", "track"); svg.appendChild(tr);
        const pu = document.createElementNS(NS, "path"); pu.setAttribute("d", d); pu.setAttribute("class", "pulse");
        pu.setAttribute("pathLength", "1000");
        pu.style.setProperty("--pc", color); pu.style.setProperty("--delay", `${(i * 0.55).toFixed(2)}s`); pu.style.setProperty("--d", `${3.2 + (i % 3) * 0.5}s`);
        svg.appendChild(pu);
      };
      $$('[data-node="in"]', beams).forEach((n, i) => add(curve(pt(n.getBoundingClientRect(), "r"), pt(hub, "l")), i, "#ffb347"));
      $$('[data-node="out"]', beams).forEach((n, i) => add(curve(pt(hub, "r"), pt(n.getBoundingClientRect(), "l")), i + 3, "#ff3b3b"));
    };
    new ResizeObserver(build).observe(beams);
  }

  /* ---------------- FAQ accordion (animated both ways, one open at a time) ---------------- */
  const faqs = $$(".faq details");
  const setFaq = (d, open) => {
    const body = $(".faq__a", d);
    const wasShown = d.open;
    const from = wasShown ? body.getBoundingClientRect().height : 0; // read mid-animation height before cancelling
    if (d._anim) d._anim.cancel();
    d.classList.toggle("is-open", open);
    if (open) d.open = true;
    const to = open ? body.scrollHeight : 0;
    if (reduced) { if (!open) d.open = false; return; }
    d._anim = body.animate([{ height: `${from}px` }, { height: `${to}px` }], { duration: 380, easing: "cubic-bezier(0.16, 1, 0.3, 1)" });
    d._anim.onfinish = () => { d._anim = null; if (!open) d.open = false; };
  };
  faqs.forEach((d) => {
    d.classList.add("js");
    d.classList.toggle("is-open", d.open);
    $("summary", d).addEventListener("click", (e) => {
      e.preventDefault();
      const open = !d.classList.contains("is-open");
      if (open) faqs.forEach((o) => o !== d && o.classList.contains("is-open") && setFaq(o, false));
      setFaq(d, open);
    });
  });

  /* ---------------- copy button ---------------- */
  $$("[data-copy]").forEach((btn) => btn.addEventListener("click", async () => {
    const text = $(btn.dataset.copy).innerText.replace(/^> /gm, "");
    try { await navigator.clipboard.writeText(text); btn.textContent = "Copied"; }
    catch { btn.textContent = "Select and copy"; }
    setTimeout(() => (btn.textContent = "Copy"), 1800);
  }));

  /* ---------------- live GitHub data ---------------- */
  const REPO = "grapedjuice/ytm-rs";
  const cached = (key, url) => {
    try {
      const hit = JSON.parse(sessionStorage.getItem(key) || "null");
      if (hit && Date.now() - hit.at < 10 * 60 * 1000) return Promise.resolve(hit.data);
    } catch {}
    return fetch(url, { headers: { Accept: "application/vnd.github+json" } })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((data) => { try { sessionStorage.setItem(key, JSON.stringify({ at: Date.now(), data })); } catch {} return data; });
  };
  // stars are fetched fresh (GitHub caches 60 s) and again when you come back to the tab, e.g. after starring
  try { sessionStorage.removeItem("ytmrs-repo"); } catch {} // stale count saved by an older version of this script
  let starsAt = 0;
  const loadStars = () => {
    if (Date.now() - starsAt < 30000) return;
    starsAt = Date.now();
    fetch(`https://api.github.com/repos/${REPO}`, { headers: { Accept: "application/vnd.github+json" }, cache: "no-cache" })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((r) => { if (typeof r.stargazers_count === "number") $$("[data-stars]").forEach((el) => (el.textContent = r.stargazers_count.toLocaleString())); })
      .catch(() => {});
  };
  loadStars();
  document.addEventListener("visibilitychange", () => { if (!document.hidden) loadStars(); });
  addEventListener("focus", loadStars);
  cached("ytmrs-release", `https://api.github.com/repos/${REPO}/releases/latest`).then((rel) => {
    if (!rel || !rel.tag_name) return;
    $$("[data-version]").forEach((el) => (el.textContent = rel.tag_name));
    const exe = (rel.assets || []).find((a) => /setup.*\.exe$/i.test(a.name));
    if (exe) {
      $$("[data-dl-direct]").forEach((a) => (a.href = exe.browser_download_url));
      $$("[data-size]").forEach((el) => (el.textContent = `${(exe.size / 1048576).toFixed(1)} MB`));
    }
    if (rel.published_at) {
      const d = new Date(rel.published_at);
      $$("[data-date]").forEach((el) => (el.textContent = d.toLocaleDateString("en-US", { month: "long", day: "numeric", year: "numeric" })));
    }
  }).catch(() => {});
})();
