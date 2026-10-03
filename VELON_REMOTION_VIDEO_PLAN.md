# Velox Launch Video — Remotion Production Plan (EXPANDED)

**Target Duration:** 180–240 seconds (3–4 minutes)  
**FPS:** 30 (standard) or 60 (premium)  
**Resolution:** 1920×1080 (Full HD), safe for 4K upscale  
**Aspect:** 16:9  
**Audio:** Voiceover (ElevenLabs male narration) + sound design + subtle music bed

---

## NARRATIVE ARC (Expanded Script)

> **[0:00–0:08] THE HOOK — The Problem**
> "I'm a web developer. I know Vue. I know React. I know CSS. Then I tried to build a native desktop app."
> 
> *Visual: Split screen — left: clean Vue SFC; right: tangled C++/Qt, Win32, or GTK code. Glitch transition between them.*

> **[0:08–0:22] THE PAIN — Traditional GUI Hell**
> "Every framework demanded a new language. New paradigms. New mental models. Qt? C++ and MOC macros. GTK? GObject boilerplate. Win32? Raw handles and message loops. Flutter? Dart, and a widget tree that fights your CSS instincts."
> 
> *Visual: Rapid-fire code montages — each framework's "Hello World" at 2× speed. Text labels: "C++", "Dart", "C", "Rust (raw)". Each flashes a "⚠ Learning Curve" badge.*

> **[0:22–0:38] THE COMPROMISE — Electron & Tauri**
> "So we turned to Electron. Web tech on desktop. Great — until you need serial ports. Bluetooth. Native menus. System tray. Hardware acceleration. You get workarounds. IPC bridges. Preload scripts. Always fragile. Always 'good enough' — never *right*. Tauri improved binary size but still requires WebView quirks, async bridges, duplicated type definitions."
> 
> *Visual: Electron architecture diagram — Chromium + Node + your app. Red "X" over "Native APIs". Tauri shows Rust backend + WebView — green check for size, yellow caution for "WebView quirks", "Async bridge", "Type duplication".*

> **[0:38–0:55] THE REVELATION — Enter Velox**
> "What if you could write native UI with *exact Vue syntax* — but compiled to Rust, running on Skia, the same engine that powers Flutter and Chrome?"
> 
> *Visual: Velox logo forms from particles. Text: "Vue Syntax · Rust Safety · Skia Rendering". Three pillars lock into place.*

> **[0:55–1:15] THE DEMO — Live Code → Live App**
> ```rust
> // App.vx — single file, zero config
> <template>
>   <div class="app">
>     <h1>{{ title }}</h1>
>     <button @click="increment">Count: {{ count }}</button>
>   </div>
> </template>
> <script setup>
> use velox_core::r#ref;
> pub struct State { count: Ref<i32> }
> impl State {
>   pub fn new() -> Self { Self { count: r#ref!(0) } }
>   pub fn title(&self) -> &str { "Velox Counter" }
>   pub fn count(&self) -> i32 { self.count.get() }
>   pub fn increment(&self) { self.count.set(self.count.get() + 1); }
> }
> </script>
> <style scoped>
> .app { padding: 24px; font-family: system-ui; }
> button { background: #0d6e66; color: white; border: none; border-radius: 8px; padding: 12px 24px; }
> </style>
> ```
> 
> *Visual: Terminal: `cargo install velox-cli && velox init myapp && cd myapp && velox dev`. Window appears. Button clicks — counter increments. Dev reload on save.*

> **[1:15–1:45] VUE DEVELOPERS, FEEL AT HOME — Code Features Walkthrough**
> "If you know Vue, you already know Velox. Let me show you."
> 
> **Reactive State** — `use velox_core::r#ref` → `r#ref!(0)` — just like `ref(0)`
> **Computed** — Methods in `impl State` that return derived values — like `computed(() => ...)`
> **Watchers** — `velox_core::watch_effect` — same API
> **Props** — Define in struct, use in template — `:prop="value"` works identically
> **Emits** — `@event="handler"` in template, define methods in `impl State` — `$emit` becomes method calls
> **Lifecycle** — `on_mounted`, `on_unmounted` — same names, same timing
> **Template Directives** — `v-if`, `v-else`, `v-for`, `v-model` — all work
> **Event Listeners** — `@click`, `@input`, `@keydown`, `@submit` — native Rust handlers
> **Scoped Styling** — `<style scoped>` — CSS modules via `data-v-*` attributes, flex/grid, variables
> 
> *Visual: Side-by-side Vue vs Velox syntax comparison for each feature. Code types in, highlights sync.*

> **[1:45–2:15] DEV EXPERIENCE COMPARISON — Velox vs Flutter vs Electron**
> "Same counter app. Three frameworks. You decide."
> 
> **Velox** — 1 file, ~50 lines, Vue syntax, native Rust, 5MB binary
> **Flutter** — 2 files (Dart + pubspec), ~80 lines, widget tree, Dart syntax, 8MB
> **Electron** — 4 files (main, preload, html, package.json), ~120 lines, IPC bridge, 150MB
> 
> *Visual: Three-panel code comparison. Lines of code counter. File count. Binary size. Syntax familiarity meter.*

> **[2:15–2:35] THE DIFFERENTIATORS — Why Velox Wins**
> | Feature | Electron | Tauri | Flutter | **Velox** |
> |---------|----------|-------|---------|-----------|
> | Language | JS/TS | Rust + JS | Dart | **Rust (Vue syntax)** |
> | Rendering | Chromium | WebView | Skia | **Skia (native)** |
> | Memory Safe | ❌ | ✅ | ✅ | **✅ (Rust)** |
> | Native APIs | IPC | Commands | Channels | **Direct Rust** |
> | Learning Curve | Low | Medium | High | **Zero (if you know Vue)** |
> | Binary Size | ~150MB | ~10MB | ~8MB | **~5MB** |
> | Single File | ❌ | ❌ | ❌ | **✅ .vx SFC** |
> | Hot Reload | ✅ | ✅ | ✅ | **Full Rebuild** |

> *Visual: Animated comparison table. Velox column highlights with glow.*

> **[2:35–2:50] THE ECOSYSTEM — It's Real**
> - CLI: `velox init`, `velox dev`, `velox build`, `velox lint`
> - VS Code / Zed / Neovim extensions
> - Reactive signals, computed, watchers — `velox_core`
> - Scoped CSS, flex/grid layout — `velox-style` + `velox-dom`
> - HMR dev loop (full rebuild, not patch)
> - MIT licensed, trademark protected
> 
> *Visual: Logo icons for each tool. Terminal commands animate in. GitHub stars badge.*

> **[2:50–3:00] HONEST DISCLAIMER**
> "Velox is in its first release — v0.1. It's under active development. Not recommended for production apps yet. But the foundation is solid, the syntax is familiar, and the direction is clear. Join us."
> 
> *Visual: Version badge "v0.1 — Early Release". GitHub issues link. Discord. Contributors welcome.*

> **[3:00–3:08] CALL TO ACTION**
> "Stop compromising. Write native apps in the syntax you already love."
> 
> `cargo install velox-cli`
> `velox init my-app`
> 
> *Visual: Velox logo + GitHub URL + Discord invite. Fade to black.*

---

## SCENE-BY-SCENE BREAKDOWN (No element static >4s)

| Scene | Time | Visual Strategy | Animation Notes |
|-------|------|-----------------|-----------------|
| **S1: Hook** | 0:00–0:08 | Split screen: Vue SFC vs Qt/GTK/Win32 | Left calm, right chaotic. Glitch wipe at 0:06. |
| **S2: Pain Montage** | 0:08–0:22 | 4 rapid code panels | 3s each. Typewriter + warning stamp. Cross-fade. |
| **S3: Architecture** | 0:22–0:38 | Electron + Tauri isometric diagrams | Layers slide in. Red/yellow pulses on pain points. |
| **S4: Reveal** | 0:38–0:55 | Particle logo → 3 pillars | Particles 1s. Pillars stagger 200ms spring. |
| **S5: Demo** | 0:55–1:15 | Terminal + Editor + Window (3-panel) | Commands type. Code scrolls. Window live. Click ripple. Reload flash. |
| **S6: Code Features** | 1:15–1:45 | **Vue ↔ Velox side-by-side** (9 features) | Each feature 3s. Sync highlight. Vue left, Velox right. |
| **S7: Dev Comparison** | 1:45–2:15 | 3-panel: Velox / Flutter / Electron | Code + file count + LOC + binary size. Animated counters. |
| **S8: Comparison Table** | 2:15–2:35 | Animated table rows | Row slide up, stagger 50ms. Velox gold sweep. |
| **S9: Ecosystem** | 2:35–2:50 | Icon grid + terminal | Stagger scale-in. Typewriter commands. Star counter. |
| **S10: Disclaimer** | 2:50–3:00 | Version badge + honest text | Fade in. Pulse badge. Links appear. |
| **S11: CTA** | 3:00–3:08 | Logo + commands + links | Pulse logo. Typewriter. Fade to black. |

---

## TECHNICAL COMPOSITION STRUCTURE (Remotion)

```tsx
// Composition tree — 5400 frames @ 30fps = 180s (3 min)
<Composition id="velox-launch" durationInFrames={5400} fps={30} width={1920} height={1080}>
  <Sequence from={0} durationInFrames={240}>       // S1 Hook (8s)
    <HookScene />
  </Sequence>
  <Sequence from={240} durationInFrames={420}>     // S2 Pain (14s)
    <PainMontageScene />
  </Sequence>
  <Sequence from={660} durationInFrames={480}>     // S3 Architecture (16s)
    <ArchitectureScene />
  </Sequence>
  <Sequence from={1140} durationInFrames={510}>    // S4 Reveal (17s)
    <RevealScene />
  </Sequence>
  <Sequence from={1650} durationInFrames={600}>    // S5 Demo (20s)
    <DemoScene />
  </Sequence>
  <Sequence from={2250} durationInFrames={900}>    // S6 Code Features (30s)
    <CodeFeaturesScene />
  </Sequence>
  <Sequence from={3150} durationInFrames={900}>    // S7 Dev Comparison (30s)
    <DevComparisonScene />
  </Sequence>
  <Sequence from={4050} durationInFrames={600}>    // S8 Comparison Table (20s)
    <ComparisonScene />
  </Sequence>
  <Sequence from={4650} durationInFrames={450}>    // S9 Ecosystem (15s)
    <EcosystemScene />
  </Sequence>
  <Sequence from={5100} durationInFrames={300}>    // S10 Disclaimer (10s)
    <DisclaimerScene />
  </Sequence>
  <Sequence from={5400} durationInFrames={240}>    // S11 CTA (8s)
    <CTAScene />
  </Sequence>
  <Audio src={staticFile("voiceover.wav")} />
  <Audio src={staticFile("music-bed.mp3")} volume={0.15} />
</Composition>
```

**Total: 5400 frames @ 30fps = 180s (3 minutes)**

---

## NEW SCENES TO BUILD

### S6: CodeFeaturesScene — Vue ↔ Velox Side-by-Side (30s)
9 feature pairs, ~3s each:
1. **Reactive State** — `ref(0)` vs `r#ref!(0)`
2. **Computed** — `computed(() => ...)` vs method returning derived
3. **Watchers** — `watchEffect(() => ...)` vs `watch_effect(|| ...)`
4. **Props** — `defineProps` vs struct fields + `:prop`
5. **Emits** — `defineEmits` + `emit()` vs `@event` + methods
6. **Lifecycle** — `onMounted` vs `on_mounted`
7. **Directives** — `v-if/v-for/v-model` identical
8. **Events** — `@click/@input` → Rust methods
9. **Scoped Styles** — `<style scoped>` identical

### S7: DevComparisonScene — Three-Way Code Compare (30s)
Three panels side-by-side:
- **Velox** (App.vx + Cargo.toml) — 1 file, ~50 LOC
- **Flutter** (main.dart + pubspec.yaml) — 2 files, ~80 LOC  
- **Electron** (main.js + preload.js + index.html + package.json) — 4 files, ~120 LOC

Animated counters: Files, Lines of Code, Binary Size, Syntax Familiarity (100% for Velox if you know Vue)

### S10: DisclaimerScene — Honest Early Release Notice (10s)
- Version badge "v0.1 — Early Release" with pulse
- Text: "Not production-ready. APIs may change. Join us on GitHub/Discord."
- Links to issues, contributing guide

---

## VOICEOVER GENERATION (ElevenLabs)

**Voice:** Male, professional narration style (e.g., "Adam" or "Antoni" — deep, confident)
**Settings:** Stability 0.5, Similarity 0.75, Style 0.2
**Model:** `eleven_multilingual_v2` for best quality
**Output:** 44.1kHz MP3 → convert to WAV for Remotion

```bash
# Using ElevenLabs API (requires API key)
curl -X POST "https://api.elevenlabs.io/v1/text-to-speech/<VOICE_ID>" \
  -H "xi-api-key: $ELEVENLABS_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "text": "<FULL_SCRIPT_HERE>",
    "model_id": "eleven_multilingual_v2",
    "voice_settings": {
      "stability": 0.5,
      "similarity_boost": 0.75,
      "style": 0.2,
      "use_speaker_boost": true
    }
  }' \
  --output voiceover.mp3

# Convert to WAV
ffmpeg -i voiceover.mp3 -ar 48000 -ac 1 voiceover.wav
```

**Voice IDs to try:** `pNInz6obpgDQGcFmaJgB` (Adam), `ErXwobaYiN019PkySvjV` (Antoni), `VR6AewLTigWG4xSOukaG` (Arnold)

---

## ASSETS NEEDED (Updated)

| Asset | Source | Specs |
|-------|--------|-------|
| Velox logo (SVG) | `/velox-logo.svg` | Use existing |
| **Voiceover (WAV)** | **ElevenLabs API** | **48kHz, mono, -16 LUFS** |
| Music bed | Epidemic Sound / Artlist | Ambient electronic, 200s loop |
| SFX: keystrokes, clicks, whoosh, pop | freesound.org | 48kHz |
| Screen recording: Velox app | `ffmpeg -f x11grab` | 1920×1080, 60fps, lossless |
| Screen recording: Terminal | Same | Crop to terminal |
| Code screenshots (Qt, GTK, Win32, Flutter, Electron) | Manual capture | 1920×1080, dark theme |
| Architecture diagrams (Electron, Tauri) | Excalidraw/Figma | Isometric, transparent BG |
| **Flutter counter code** | Create from Flutter docs | `main.dart` + `pubspec.yaml` |
| **Electron counter code** | Create from Electron docs | 4 files |

---

## NEXT STEPS (Updated)

1. **Generate voiceover** — Use ElevenLabs API with full script above
2. **Create Flutter/Electron comparison code** — Scaffold minimal counter apps
3. **Record screen captures** — Velox app, terminal, editor, Flutter, Electron
4. **Build new scenes** — `CodeFeaturesScene`, `DevComparisonScene`, `DisclaimerScene`
5. **Extend composition** — Update timing, add sequences
6. **Render + deliver**

---

## ESTIMATED EFFORT (Updated)

| Phase | Hours |
|-------|-------|
| Project setup + asset prep | 6 |
| Scene components (11 scenes) | 28 |
| Animation polish + timing | 12 |
| Voiceover generation + sync | 4 |
| Screen recording (Velox, Flutter, Electron) | 6 |
| Audio mix + SFX | 4 |
| QA + renders (draft → final) | 6 |
| **Total** | **~66 hours** |