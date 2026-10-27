# egui / eframe: the version the binary was built with, and the API Hydra calls

Phase 2 research deliverable (job `egui`). Everything below was read out of the
arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra` (unslid VAs), out of
`symbols_demangled.txt` / `nm -a … | rustfilt`, or out of the egui source at the
recovered commit. No code, manifest or lock file was changed by this job; the
only artefact is this file.

---

## 0. Verdict

| question | answer | confidence |
|---|---|---|
| which source? | `github.com/emilk/egui`, **git** dependency (not a crates.io release) | high |
| which rev? | **`3da022d98072fb631443a72f49973cfb6c93f865`** (cargo prints `3da022d`) | high |
| which version? | all seven crates carry `version = "0.35.0"` in that rev's `[workspace.package]`; the rev is a 0.35.0-line revision, **not** the `0.35.0` release tag (`f72eaf6` is not an ancestor) | high for the label, medium for "which release is nearest" |
| eframe renderer | **wgpu** (`egui-wgpu`), no `egui_glow` anywhere in the image | high |
| features proven on | `default_fonts` (the bundled-face name strings `Ubuntu-Light`, `NotoEmoji-Regular`, `emoji-icon-font` are in `__const`), `clipboard` (arboard), `links` (webbrowser), `wgpu`+`capture`, winit integration | high |
| features proven **off** | `accesskit` (no `accesskit_winit`/`accesskit_consumer` at all), `persistence` (no `ron`, no `home`), `glow` | high |

Practical consequence: the egui family **must** be pinned by `git` + `rev`, not by
version. `hydra_gui` and `gs_render` call things that only exist on this branch of
the checkout — see §3 and §2.4.

---

## 1. Evidence inventory

### 1.1 cargo's git checkout path (the anchor)

`strings -a` on the binary yields **64 distinct** source-file paths of the form

```
/Users/nisugi/.cargo/git/checkouts/egui-5b6373fea415a27f/3da022d/crates/<crate>/src/…
```

Cargo names a git checkout directory `<repo-name>-<url-hash>/<short-rev>/`, so the
pair (`5b6373fea415a27f`, `3da022d`) *is* cargo's record of the URL identity and
the exact resolved commit. Per crate (unique files):

| crate dir | files | examples |
|---|---|---|
| `crates/egui` | 35 | `context.rs`, `ui.rs`, `plugin.rs`, `style.rs`, `grid.rs`, `hit_test.rs`, `layers.rs`, `memory/mod.rs`, `response.rs`, `widget_rect.rs`, `atomics/atom_layout.rs`, `containers/{area,collapsing_header,menu,resize,scroll_area,tooltip,window}.rs`, `data/input/raw_input.rs`, `input_state/touch_state.rs`, `load/{bytes_loader,texture_loader}.rs`, `text_selection/{accesskit_text,cursor_range,text_cursor_state,visuals}.rs`, `util/{id_type_map,undoer}.rs`, `widgets/{image,label,progress_bar,slider,spinner}.rs`, `widgets/text_edit/{builder,text_buffer}.rs` |
| `crates/epaint` | 11 | `image.rs`, `mesh.rs`, `shapes/shape.rs`, `tessellator.rs`, `texture_atlas.rs`, `viewport.rs`, `text/{font,fonts,index,text_layout,text_layout_types}.rs` |
| `crates/eframe` | 6 | `lib.rs`, `native/{app_icon,epi_integration,event_loop_context,run,wgpu_integration}.rs` |
| `crates/egui-wgpu` | 5 | `capture.rs`, `lib.rs`, `renderer.rs`, `setup.rs`, `winit.rs` |
| `crates/ecolor` | 3 | `color32.rs`, `hsva.rs`, `lib.rs` |
| `crates/egui-winit` | 2 | `clipboard.rs`, `lib.rs` |
| `crates/emath` | 2 | `lib.rs`, `rect.rs` |

Two things follow immediately from the paths themselves:

* the repository layout at that rev has `crates/{egui,eframe,epaint,ecolor,emath,egui-wgpu,egui-winit,…}`
  (i.e. the modern egui monorepo, after `ecolor`/`emath` were split out of `epaint`);
* the same `strings` sweep shows Hydra's *other* git dependencies, which confirms
  the tool reading: `gs_studio-539b52d288f26158/c1b83bf/crates/{gs_puppet,gs_render}/…`
  and `hydra-mapper-e0c8e5dc4917a4de/defeb84/crates/map-layout/…`. Same
  `<name>-<hash>/<shortrev>` scheme.

### 1.2 How many egui symbols there are

| counting method | egui | egui-wgpu | egui-winit | eframe | epaint | ecolor | emath |
|---|---|---|---|---|---|---|---|
| text symbols in `symbols_demangled.txt` naming the crate | 343 | 29 | 21 | 17 | 83 | 12 | 11 |
| `nm -a` (global **+ local**) lines naming the crate | 2110 | 76 | 48 | 102 | 307 | 14 | 30 |

`symbols_demangled.txt` holds 10 694 lines of which **393** name an egui-family
crate — a single line can name two of them, e.g.
`<egui::viewport::IconData as eframe::icon_data::IconDataExt>::to_image`.
`nm -a | rustfilt` gives 120 559 symbols, **2 166** of which name an egui-family
crate; the excess over the globals is monomorphised egui generics placed in the
app's crates plus egui's own local (`t`) functions.

### 1.3 Salient symbols (unslid VA)

| symbol | address | why it matters |
|---|---|---|
| `eframe::run_native` | `0x100b1c654` | the app's entry into eframe |
| `eframe::run_native_ext` | `0x100b1c7a0` | 3-arg `run_native` delegates here (`egui_ctx: Option<egui::Context>`) |
| `<hydra_gui::app::App as eframe::epi::App>::ui` | `0x1004f79a0` | the app's `App` impl (note: **`ui`**, not `update`) |
| `eframe::epi::Frame::set_key_capture` | `0x100b2bca0` | branch-only API (see §2.4) |
| `egui::context::Context::set_viewport_zoom_factor` | `0x100ea8948` | added *by* the checkout commit itself |
| `egui::context::Context::viewport_zoom_factor` | `0x100ea465c` | ditto |
| `egui_wgpu::renderer::Renderer::new` | `0x100c04aac` | wgpu renderer is compiled in |
| `egui_wgpu::winit::Painter::paint_and_update_textures` | `0x100c0113c` | winit integration |
| `egui_wgpu::WgpuConfiguration as Default>::default` | `0x100c07868` | called from `hydra_gui::app::run` |
| `<gs_render::field_gpu::FieldCallback as egui_wgpu::renderer::CallbackTrait>::{prepare,finish_prepare,paint}` | `0x100963924`, `0x1009635bc`, `0x1009636f0` | gs_render's paint callback |
| `egui_wgpu::renderer::Callback::new_paint_callback` | `0x1009602dc` | `epaint::PaintCallback` construction inside gs_render |
| `<hydra_gui::bar::Bar as egui::widgets::Widget>::ui` | `0x1006f4920` | Hydra implements `egui::Widget` |
| `egui::ui::Ui::available_size` | `0x100f2a930` | the one egui call inside `Bar::fitted` |
| `egui::style::TextStyle::resolve` | `0x100f0bf14` | caption measuring in `Bar::fitted` |
| `egui_winit::State::new` | `0x100b3cbdc` | egui-winit `State::new` (signature is version-sensitive) |

The `egui-wgpu` WGSL shader is embedded verbatim in `__TEXT,__const` and is a
second independent version fingerprint. Salient literals at `0x2223…`:

```
struct Locals { screen_size: vec2<f32>, dithering: u32,
                /// 1 to do manual filtering for more predictable kittest snapshot images.
                /// See also https://github.com/emilk/egui/issues/5295
                predictable_texture_filtering: u32 }
…
egui_index_buffer / egui_vertex_buffer
Unknown paint callback: expected `egui_wgpu::Callback`
        // egui_wgpu::renderer
Mismatch between texture size and texel count
Tried to update a texture that has not been allocated yet.
Tried to update user texture.
```

`predictable_texture_filtering` is a field of `egui_wgpu::RendererOptions`
("Perform texture filtering in software … e.g. for kittest snapshots"), documented
for egui-wgpu 0.33/0.34
(<https://docs.rs/egui-wgpu/0.34.3/egui_wgpu/struct.RendererOptions.html>), and it
came in with the kittest "predictable sampling" change described in
<https://github.com/emilk/egui/issues/7630>. That places egui-wgpu at **≥ 0.33**,
which is consistent with — and much weaker than — the exact-rev determination of §2.
The `Unknown paint callback` message proves the `Painter::add(…PaintCallback…)`
path really runs through `egui_wgpu::Renderer`, i.e. the wgpu backend was the one
in use.

### 1.4 Panic `Location` paths

Every `panic_bounds_check`/`slice_index_order_fail` carries a `(&str, len, line, col)`
record, so the file names survive without DWARF. **64 distinct** egui-family files
appear, all rooted at the checkout above:

| crate | files |
|---|---|
| `egui` | 35 |
| `epaint` | 11 |
| `eframe` | 6 |
| `egui-wgpu` | 5 |
| `ecolor` | 3 |
| `egui-winit`, `emath` | 2 each |

Full list — every one of the 64 paths exists in a checkout of `3da022d` (scripted
check, 0 misses):

```
crates/ecolor/src/{color32,hsva,lib}.rs
crates/eframe/src/lib.rs
crates/eframe/src/native/{app_icon,epi_integration,event_loop_context,run,wgpu_integration}.rs
crates/egui-wgpu/src/{capture,lib,renderer,setup,winit}.rs
crates/egui-winit/src/{clipboard,lib}.rs
crates/egui/src/{context,grid,hit_test,layers,plugin,response,style,ui,widget_rect}.rs
crates/egui/src/atomics/atom_layout.rs
crates/egui/src/containers/{area,collapsing_header,menu,resize,scroll_area,tooltip,window}.rs
crates/egui/src/data/input/raw_input.rs
crates/egui/src/input_state/touch_state.rs
crates/egui/src/load/{bytes_loader,texture_loader}.rs
crates/egui/src/memory/mod.rs
crates/egui/src/text_selection/{accesskit_text,cursor_range,text_cursor_state,visuals}.rs
crates/egui/src/util/{id_type_map,undoer}.rs
crates/egui/src/widgets/{image,label,progress_bar,slider,spinner}.rs
crates/egui/src/widgets/text_edit/{builder,text_buffer}.rs
crates/emath/src/{lib,rect}.rs
crates/epaint/src/{image,mesh,tessellator,texture_atlas,viewport}.rs
crates/epaint/src/shapes/shape.rs
crates/epaint/src/text/{font,fonts,index,text_layout,text_layout_types}.rs
```

Two of these are *version* evidence on their own: `crates/egui/src/plugin.rs` and
`crates/egui/src/atomics/atom_layout.rs` — the plugin system and the atoms/atom-layout
module are recent egui (`egui::plugin::{Plugin, Plugins}`, `egui::atomics::{Atom, Atoms, AtomLayout}`).

### 1.5 Version strings: there are none, and that is fine

`strings -a | grep -E '^0\.3[0-9]\.[0-9]$'` → **0 hits**. There is no `egui 0.35.0`
banner, no `CARGO_PKG_VERSION` of egui in the image; the only version strings are
Hydra's own (`hydra/0.1.0-beta.7`, `dev.hydrafe.desktop`). The version therefore
has to be determined structurally (§2), not by reading a literal.

Registry-path strings for the *other* pinned deps do exist and are the cross-check
of §2.3, e.g. `…/registry/src/index.crates.io-1949cf8c6b5b557f/{wgpu-29.0.4,
wgpu-core-29.0.4, winit-0.30.13, harfrust-0.7.0, skrifa-0.42.1, vello_cpu-0.0.9,
kurbo-0.13.1, accesskit-0.24.1, arboard-3.6.1, …}/…`.

### 1.6 `3da022d` + `5b6373fea415a27f` → which source?

1. **The short rev is the exact commit.** Cargo materialises one directory per
   locked revision (`…/checkouts/<ident>/<first-7-of-sha>/`), so the dependency's
   `rev`/lock entry is `3da022d…`, not "somewhere after it".
2. **The full sha.** `https://api.github.com/repos/emilk/egui/commits/3da022d`
   returns `sha = 3da022d98072fb631443a72f49973cfb6c93f865`, author *Nisugi*
   (the Hydra author), date `2026-10-20T09:00:01Z`, message
   `feat(egui): a zoom factor per viewport`, parent `ed8b2649fd4b1c3351e38988b045f1bf33335f38`;
   the diff touches exactly `crates/egui-winit/src/lib.rs` and
   `crates/egui/src/context.rs` and **adds `Context::set_viewport_zoom_factor` and
   `Context::viewport_zoom_factor`** — both of which are in the binary (§1.3).
   (<https://github.com/emilk/egui/commit/3da022d98072fb631443a72f49973cfb6c93f865>)
3. **Negative control.** The same lookup against an unrelated repo,
   `api.github.com/repos/gfx-rs/wgpu/commits/3da022d…`, answers
   `422 No commit found for SHA`, so step 2 is not a wildcard match.
4. **The parent pins a floor.** `ed8b2649…` is `feat(eframe): app-configurable key
   capture before egui-winit`, which is what introduces
   `Frame::set_key_capture` / `Frame::captured_keys` / numpad interception; the
   binary calls `Frame::set_key_capture` and `Frame::set_numpad_capture_keys`
   from `hydra_gui::app::App::ui`. So the build is *at least* `ed8b264`, and
   cargo's directory name says *exactly* `3da022d`.
5. **The tree matches.** All 64 `Location` paths of §1.4 exist in the rev, and all
   244 distinct `crate::path` mentions in the symbol table resolve to a symbol
   that exists in that rev's sources (scripted check: 0 misses).

What is **not** recoverable locally is the exact URL cargo hashed to
`5b6373fea415a27f` — the ident hash uses cargo's internal hasher and the matching
`~/.cargo/git` database does not exist on this machine (checked). §5 gives a
one-minute experiment that turns this into a positive check.

### 1.7 Features the crates prove, and features they rule out

| probe | evidence | conclusion |
|---|---|---|
| `egui_glow` symbols / strings | 0 hits | `glow` off; wgpu only |
| `accesskit_winit`, `accesskit_consumer` | 0 hits; `accesskit-0.24.1` *is* present | `eframe/accesskit` (→ `egui-winit/accesskit`) **off**; the `accesskit` you see is egui's own non-optional dep (`ContextImpl::accesskit_node_builder`, `Response::fill_accesskit_node_from_widget_info`, `text_selection::accesskit_text`) |
| `arboard::*` symbols (`clipboard::Clipboard::{new,get,set_text,set_image}`) | present, and `arboard-3.6.1` in the registry paths | `egui-winit/clipboard` **on** |
| `webbrowser` | `webbrowser::*` symbols ×5, `webbrowser-1.2.4` in the registry paths, `Context::open_url`, `OpenUrl::new_tab` | `egui-winit/links` **on** |
| `ron`, `home` | 0 hits; `eframe::get_value`/`set_value` absent | `eframe/persistence` **off** (`App::save`/`Frame::storage` symbols exist regardless — they are not `cfg`-gated) |
| bundled fonts | `NotoEmoji-Regular`, `Ubuntu-Light`, `emoji-icon-font` in `__const` | `default_fonts` **on** (Hydra adds its own faces on top: `hydra_gui::fonts::Fonts::give` → `FontDefinitions::default` + `FontData::{from_static,from_owned}`) |
| `egui_wgpu::capture::CaptureState` + `handle_screenshots` | present | `egui-wgpu/capture` on (eframe always enables it) |
| `egui_extras`, `egui_kittest`, `egui_inspection`, `rfd`, `puffin` | no symbols; the only `egui_extras` text is inside egui's own loader error message ("…e.g. using egui_extras.") | not in the build |

---

## 2. Version determination

### 2.1 The rev

`git rev = 3da022d98072fb631443a72f49973cfb6c93f865` on `https://github.com/emilk/egui`.
Confidence **high**: it is cargo's own record (§1.6.1), the commit introduces two
functions the binary calls (§1.6.2), and its parent introduces two more (§1.6.4).

### 2.2 The labelled version

The rev's root `Cargo.toml` has

```toml
[workspace.package]
edition = "2024"
rust-version = "1.92"
version = "0.35.0"
```

so `egui`, `epaint`, `ecolor`, `emath`, `egui-wgpu`, `egui-winit` and `eframe` all
resolve as **0.35.0** and cargo prints them as
`egui v0.35.0 (https://github.com/emilk/egui?rev=3da022d…#3da022d98)`.

That label is *not* the `0.35.0` release: GitHub's compare of
`0.35.0 → 3da022d` reports `merge_base = a08630c9…` (2026-06-25, just before the
release commit `f72eaf6` "Add egui_inspection to missing places", 0.35.0 tagged
`2026-06-25`), i.e. the commit sits on a branch cut before that release commit and
carried forward by Nisugi's own commits (`ed8b264`, 2026-09-29; `3da022d`,
2026-10-20). The tree still says `0.35.0`; the next release tag, `0.36.0`, is
`a4edc4a9…` (2026-08-05) and *does* bump the version. So the branch was cut before
the `0.35.0` release and never picked that version bump up, which is why a commit
dated October still declares `0.35.0`.

Narrowest defensible statement: **a 0.35.0-line git revision of `emilk/egui`
(post-`0.34.3`, version label `0.35.0`), with the author's own patches on top.**
"which crates.io release is nearest" is not determinable and does not matter —
nothing in the graph may come from crates.io.

### 2.3 The dependency fingerprint (the strongest single argument)

The rev's `[workspace.dependencies]` pins and the rev's own `Cargo.lock` compared
with what the binary actually links (`crate_versions.txt`). The lock differs from
the binary in exactly the patch positions you would expect from a later resolve:

| crate | egui `Cargo.toml` @3da022d | egui `Cargo.lock` @3da022d | binary (`crate_versions.txt`) |
|---|---|---|---|
| `wgpu` | `29.0` | 29.0.3 | **29.0.4** |
| `winit` | `0.30.13` | **0.30.13** | **0.30.13** |
| `accesskit` | `0.24.1` | **0.24.1** | **0.24.1** |
| `arboard` | `3.6` | **3.6.1** | **3.6.1** |
| `bytemuck` | `1.25` | 1.25.0 | 1.25.2 |
| `harfrust` | **`0.7.0`** | **0.7.0** | **0.7.0** |
| `skrifa` | **`0.42.1`** | **0.42.1** | **0.42.1** |
| `font-types` | **`0.11.3`** | **0.11.3** | **0.11.3** |
| `read-fonts` | (via skrifa) | **0.39.2** | **0.39.2** |
| `vello_cpu` / `vello_common` | **`0.0.9`** | **0.0.9** | **0.0.9** |
| `kurbo` | (via vello) | 0.13.1 | **0.13.1** |
| `pollster` | **`0.4.0`** | **0.4.0** | **0.4.0** |
| `objc2` / `objc2-app-kit` | 0.6.4 / 0.3.2 | 0.6.4+0.5.2 / 0.3.2+0.2.2 | 0.6.4+0.5.2 / 0.3.2+0.2.2 |
| `image` | `0.25.6` | 0.25.6 | 0.25.10 |
| `webbrowser` | `1.2` | 1.2.1 | 1.2.4 |

Four of these are *hard*, non-guessable discriminators that independently pin the
generation of epaint: `harfrust 0.7.0` (HarfBuzz-based shaping), `skrifa 0.42.1` +
`read-fonts 0.39.2` + `font-types 0.11.3` (skrifa replaced `ab_glyph`; note the
rev's own comment in `Cargo.toml` explaining why these cannot be bumped), and
`vello_cpu 0.0.9` (glyph rasterisation — the binary has
`<epaint::text::font::VelloPen as read_fonts::model::pen::OutlinePen>::*`).
The binary has no `ab_glyph` at all.

### 2.4 API/signature discriminators

* `eframe::epi::App` at this rev is `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame)`
  (plus an optional `fn logic(...)`) — the binary's only `App` method symbol is
  `<hydra_gui::app::App as eframe::epi::App>::ui`. An `update(&mut self, ctx, frame)`
  era eframe cannot produce that symbol.
* `eframe::epi::Frame::set_key_capture(HashSet<winit::keyboard::KeyCode>)` and
  `set_numpad_capture_keys(...)` exist here (added by the rev's parent) and are
  called.
* `egui::context::Context::{set_viewport_zoom_factor, viewport_zoom_factor}` exist
  here (added *by* the rev) and are called.
* `egui::plugin::{Plugin, Plugins::{add, get}, CallbackPlugin, DebugTextPlugin}`,
  `Context::{plugin, with_plugin}`, `drag_and_drop::DragAndDrop as Plugin`,
  `text_selection::label_text_selection::LabelSelectionState as Plugin`,
  `atomics::{Atom, Atoms, AtomLayout, SizedAtomLayout, AtomKind}`,
  `containers::menu::{MenuButton, SubMenuButton, MenuConfig, MenuState}`,
  `containers::panel::{Panel, CentralPanel}`, `id_salt::IdSalt`, `ui_builder::UiBuilder`,
  `pass_state::PassState`, `context::ContextImpl` — all present in the rev's source
  and all present in the symbol table.
* `egui_wgpu::setup::{WgpuSetup, WgpuSetupCreateNew, EguiDisplayHandle}` (the
  `wgpu_setup` field of `WgpuConfiguration`) and `egui_wgpu::winit::Painter` with
  `gc_viewports`/`install_surface` exist; `egui_winit::window_settings::WindowSettings`
  exists (its `initialize_window`/`initialize_viewport_builder` are separate symbols).
* Layout check on `egui::Key`: the variant-name table at `0x1043947b0` (lengths as
  `u64` at `0x102140490`) starts `ArrowDown, ArrowLeft, ArrowRight, ArrowUp,
  Escape, Tab, …` and its **first 117 entries are exactly the 117 variants of
  `egui::Key` in declaration order at `3da022d`** — index 53 = `H`, 54 = `I`,
  116 = `IntlBackslash` (verified against `crates/egui/src/data/key.rs`).
  A further 66 names follow in the *same* pointer run (`LaunchApp1, LaunchApp2,
  LaunchMail, MediaPlayPause, …, Again, Copy, Cut, Find, Open, Paste, Props,
  Select, Undo, Hiragana, Katakana, F1…F35`) — that is winit's `KeyCode` name
  table sitting immediately after it in `__DATA_CONST`, not more egui keys.
  *Audit note for Phase 4:* `crates/hydra_gui/NOTES.md` records this table as
  "138 entries, `ArrowDown … Again`" and therefore reads
  `keys::names::capturable`'s five codes as `LaunchMail/LaunchApp2/LaunchApp1/H/I`;
  the first three (`0x75/0x76/0x77` = 117/118/119) are **outside** `egui::Key`'s
  0…=116 range and are almost certainly an artefact of that table crossing its
  own boundary. Re-derive `0x101ee2a08` before trusting it.
* `egui::Event` is a 72-byte value in this build (`hydra_gui::keys::Keys::take`
  retains over a `Vec` with a 0x48 stride) — a hard layout constraint on any pin.

### 2.5 Confidence and residual unknowns

* rev: **high** (§2.1).
* version label `0.35.0`: **high** (it is in the rev's own manifest).
* "nearest released version": **not determinable**, and it must not be used. The
  binary calls `Context::set_viewport_zoom_factor` and `Frame::set_key_capture`,
  which are introduced by commits on this branch (`3da022d` and its parent
  `ed8b264`), and `3da022d` is **not** an ancestor of the `0.35.0` tag (GitHub's
  compare reports merge base `a08630c`), so no released tarball can be substituted
  for the rev.
* URL identity `https://github.com/emilk/egui`: **high**, resting on §1.6.2/3
  (commit resolves in `emilk/egui`, not in an unrelated repo). The one loose end
  is that cargo's ident hash `5b6373fea415a27f` cannot be recomputed offline; §5
  shows how to settle it in one command. If it does not match after a fetch, the
  dependency is a fork/mirror of that repo at the same rev — the `rev` is
  unaffected, only the URL in the manifest.
* No `git` history trickery is assumed: a shallow fetch of the sha is enough (and
  is exactly what cargo keeps).

---

## 3. API surface actually called by the five crates

Method: the full `__TEXT` disassembly (`objdump -d --no-show-raw-insn`, 482 MB)
was scanned for `bl` edges; each edge was attributed to the caller by the nearest
preceding text symbol in `nm -a | rustfilt` (local symbols included, ICF-aware),
and the callee likewise. Result: **1 940 edges** from the five crates into
egui-family symbols, **250 distinct callees**.

| crate | distinct egui-family callees | note |
|---|---|---|
| `hydra_gui` | **248** | the whole UI |
| `gs_render` | 7 | paint callback + painter |
| `hydra_board` | 2 | a texture handle and one layout query |
| `hydra_ui` | 0 (one drop-glue instantiation) | nothing survives inlining |
| `gs_puppet` | 0 | std-only by design |

`R` = **required** for the recovered call sites to compile; `O` = used but
replaceable/nice-to-have.

### 3.1 `hydra_gui` — `egui`

**`egui::Context` (R)** — `content_rect`, `copy_text`, `cumulative_pass_nr`,
`global_style`, `layer_id_at`, `layer_painter`, `layer_transform_from_global`,
`load_texture`, `move_to_top`, `open_url`, `plugin`, `read`, `request_discard`,
`request_repaint`, `request_repaint_after`, `request_repaint_of`,
`send_viewport_cmd`, `send_viewport_cmd_to`, `set_cursor_icon`, `set_fonts`,
`set_style_of`, `set_sublayer`, **`set_viewport_zoom_factor`**, `show_viewport_immediate`,
`system_theme`, `text_edit_focused`, `theme`, `viewport_id`, `viewport_rect`,
**`viewport_zoom_factor`**, `with_plugin`, `write` (the last two are the plugin
handle path: `ui.ctx().plugin::<T>()`). `ContextImpl::{pixels_per_point, viewport,
viewport_for}` are reached through `Context`'s delegating wrappers.

**`egui::Ui` (R)** — `add_enabled`, `add_sized`, `advance_cursor_after_rect`,
`allocate_exact_size`, `allocate_space`, `allocate_ui_with_layout_dyn`,
`available_height`, `available_rect_before_wrap`, `available_size`,
`available_size_before_wrap`, `available_width`, `close`, `color_edit_button_srgb`,
`colored_label`, `disable`, `end_row`, `heading`, `horizontal_top`,
`horizontal_with_main_wrap_dyn`, `interact`, `is_rect_visible`, `new_child`,
`next_auto_id`, `painter_at`, `push_id`, `put`, `radio_value`,
`rect_contains_pointer`, `scope`, `scope_dyn`, `scroll_to_cursor`,
`scroll_to_rect_animation`, `scroll_with_delta_animation`, `set_max_width`,
`set_min_size`, `small_button`, `spacing_mut`, `strong`, `style_mut`,
`text_style_height`, `toggle_value`, `weak`, plus `impl Drop for Ui`.

**`egui::Response` (R)** — `dnd_release_payload`, `dnd_set_drag_payload`,
`has_focus`, `interact`, `labelled_by`, `lost_focus`, `on_hover_ui`,
`on_hover_ui_at_pointer`, `request_focus`, `scroll_to_me`, `union`, `widget_info`.

**`egui::Painter` (R)** — `add`, `extend`, `image`, `text`.

**Containers (R)** — `Area::{new, show}`; `CollapsingHeader::{new, show_dyn}` +
`CollapsingState::{load, load_with_default_open, store, toggle}` +
`HeaderResponse::body`; `ComboBox::{show_ui, show_ui_dyn}`;
`Frame::{begin, show_dyn}` + `Prepared::end`; `MenuButton::{new, ui}`,
`SubMenuButton::{new, ui}`, `MenuConfig::new`, `menu::is_in_menu`;
`CentralPanel::show_inside_dyn`, `Panel::show_inside_dyn`;
`Popup::{context_menu, is_any_open, show}`; `ScrollArea::show_viewport_dyn`;
`Tooltip::{for_enabled, for_disabled, show}`; `Window::{new, show_dyn}`.

**Widgets (R)** — `Button::{new, opt_image_and_text}` + `Widget for Button`,
`Checkbox::{without_text}` + `Widget`, `DragValue::{new}` + `Widget`,
`Image::{new, new_mono}` + `Widget`, `Label::{layout_in_ui}` + `Widget`,
`ProgressBar::{new}` + `Widget`, `Separator` (+ `Default`) + `Widget`,
`Slider` + `Widget`, `TextEdit::{load_state, show}` + `Widget` +
`text_edit::state::TextEditState::{load, store, clear_undoer}` + `TextEditOutput`,
`Hyperlink`/`Link` + `Widget`, `RadioButton` + `Widget`, `Spinner` + `Widget`;
`WidgetText`, `RichText::{new, Default}`, `egui::Widget` itself (Hydra implements
it for `bar::Bar`).

**Atomics (R on this rev — the whole UI is built on them)** —
`Atoms::{text, extend_right}`, `IntoAtoms::{into_atoms, collect}`, `Atom`,
`AtomKind::into_sized`, `Atom::into_sized`, `AtomLayout::{measure, allocate}`,
`SizedAtomLayout::paint_at`.

**Input / state (R)** — `InputState::key_pressed`,
`PointerState::{any_down, any_pressed, button_clicked, button_double_clicked,
primary_pressed, secondary_clicked}`, `Modifiers::matches_exact` (used by
`keys::Chord` matching), `ViewportInfo::close_requested`,
`data::output::OpenUrl::new_tab`, `Memory::{focus_mut, move_focus}`,
`util::id_type_map::IdTypeMap::{get_temp, insert_temp, remove}`,
`Id::{new, with}`, `IdSalt::new`, `Placer::advance_cursor`,
`drag_and_drop::DragAndDrop::set_payload`,
`text_selection::label_text_selection::LabelSelectionState::label_text_selection`,
`data::input::event::Event` (72-byte value), `data::output::WidgetInfo`.

**Style / text (R)** — `Style::{interact, Clone}`, `Style::default`,
`Visuals::{dark, light, weak_text_color}`, `TextStyle::resolve`,
`style::FontSelection::resolve_with_fallback`, `widget_style::{button_style, widget_style}`.

**Grid / layout (R)** — `Grid::{show, show_dyn, striped}`.

### 3.2 `hydra_gui` — `epaint`, `ecolor`, `emath`

* `epaint::ColorImage::{from_rgba_unmultiplied, filled}` (R) — `pictures`, `story_picture`.
* `epaint::TextureHandle::{size, size_vec2}` + `Clone`/`Drop` (R) — `pictures`, `widget::{doll, hotbar, draw}`, `compass_art`, `hotbar_editor::icon`, `theme_editor::packs`.
* `epaint::text::fonts::{FontData::{from_static, from_owned}, FontDefinitions}` (R) — `fonts::Fonts::give`.
* `epaint::text::font::Font::styled_metrics`, `text::fonts::FontsImpl::font` (R) — `play::draw::holder` measures text itself.
* `epaint::text::text_layout_types::{LayoutJob::append, TextFormat, Galley::{cursor_from_pos, pos_from_cursor}}` (R) — `text::job`, `text::linked_in`, `widget::roster`, `highlight_editor`.
* `epaint::shapes::shape::dashes_from_line` (O) — `pad::wheel_page`, `widget::minimap`.
* `epaint::{Mesh, Shape}` (R, as values), `epaint::texture_handle::TextureHandle` in `Option`.
* `ecolor::Color32::{lerp_to_gamma, from_hex}` + `hex_color_runtime::HexColor` `Display` (R) — `logs::lines`, `theme_editor`.
* `emath::Align2::align_size_within_rect` (R) — `bar::Bar::ui`; `emath::{Pos2, Rect, Vec2}` everywhere.

### 3.3 `hydra_gui` — `eframe`, `egui-wgpu`, `egui-winit`

* `eframe::run_native` + `run_native_ext` (R) — `app::run`. Actual signature at the rev:
  `run_native(app_name: &str, native_options: NativeOptions, app_creator: AppCreator) -> Result`
  (`NativeOptions` is passed **by value**, which is why the call site in `app::run`
  copies a large struct through several registers plus the stack).
* `eframe::epi::App` (R) — Hydra implements **`fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame)`**;
  no `logic` override is present (only the default vtable entries `auto_save_interval`,
  `clear_color`, `on_exit`, `persist_egui_memory`, `raw_input_hook`, `save`).
* `eframe::epi::NativeOptions` (R) — `Default::default()`, then the viewport
  (`"Hydra"`, 660×520) and `wgpu_options` are set; `drop_in_place<NativeOptions>`
  and `drop_in_place<egui_wgpu::WgpuConfiguration>` appear in `app::run`.
* `eframe::epi::CreationContext` (R) — only as the `AppCreator` closure argument:
  `Box<dyn FnOnce(&CreationContext) -> Result<Box<dyn App>, Box<dyn Error + Send + Sync>>>`.
* `eframe::epi::Frame` (R) — `set_key_capture`, `set_numpad_capture_keys`
  (and implicitly `wgpu_render_state()`: `Option<egui_wgpu::RenderState>` drop glue).
* `eframe::epi::Storage` (O) — `Option<Box<dyn Storage>>` drop glue only; the
  `persistence` feature is off, so this is always `None`.
* `eframe::Error` (R) — `run_native`'s error type, `Display` + `Result<(), Error>` drops.
* `eframe::icon_data::IconDataExt::to_image` (R, inside eframe) — eframe converts
  the `egui::IconData` set on `NativeOptions::viewport` → `ViewportBuilder::icon`
  (built by `hydra_gui::badge::pixels`, 256×256 RGBA) into an `image::RgbaImage`,
  which is why eframe needs `image` with the `png` feature.
* `egui_wgpu::WgpuConfiguration::default()` (R) — assigned to `NativeOptions::wgpu_options`.
* `egui_wgpu::{Renderer, RenderState, setup::WgpuSetupCreateNew, winit::Painter,
  capture::CaptureState}` — reached by eframe, not called by Hydra; listed because
  they must exist in the build.

### 3.4 `gs_render` (7 callees)

* `egui::painter::Painter::{add, extend}` (R) — `field_gpu::paint_view`,
  `puppet_cache::PuppetPlacement::paint_figure`.
* `egui::painter::Painter::image` (R) — `PuppetPlacement::paint_figure`.
* `egui::context::Context::write` (R) — `field_gpu::init` (state into `Context` data).
* `egui_wgpu::renderer::Callback::new_paint_callback(rect, cb) -> epaint::PaintCallback` (R) — `field_gpu::paint_view`.
* `egui_wgpu::renderer::CallbackTrait` (R) — implemented by
  `gs_render::field_gpu::FieldCallback` with `prepare`, `finish_prepare`, `paint`:
  `prepare(&self, &wgpu::Device, &wgpu::Queue, &ScreenDescriptor, &mut wgpu::CommandEncoder, &mut CallbackResources) -> Vec<wgpu::CommandBuffer>`,
  `finish_prepare(&self, &Device, &Queue, &mut CommandEncoder, &mut CallbackResources) -> Vec<CommandBuffer>`,
  `paint(&self, info: PaintCallbackInfo, render_pass: &mut wgpu::RenderPass<'static>, callback_resources: &CallbackResources)`.
* `epaint::texture_handle::TextureHandle` (R) — cached, and `(TextureHandle, [f32; 2], [usize; 2])`
  / `(TextureHandle, Arc<[f32]>)` tuples appear in gs_render/hydra_gui caches.
* `epaint::PaintCallback` (R) — the value `new_paint_callback` returns and
  `Painter::add` consumes.

### 3.5 `hydra_board` (2 callees, one generic seam)

* `epaint::texture_handle::TextureHandle` (R) — `disk::Disk::kept_at` is generic over
  the cached value and is instantiated at `TextureHandle` and
  `(TextureHandle, Arc<[f32]>)` (drop glue names both, from `hydra_gui::pictures`).
* `egui::ui::Ui::available_width` (R) — `targets::Target::named`.
* `hydra_gui`'s `EguiWindows` implements `hydra_board::seat::Windows` and forwards
  `open_url`/`repaint` to `egui::Context::{open_url, request_repaint}`.

### 3.6 `hydra_ui`, `gs_puppet`

* `hydra_ui`: **no surviving egui call edge**. The only occurrence is a drop-glue
  instantiation of `egui::containers::tooltip::Tooltip::show<(), hydra_gui::…::hover::{{closure}}>::{{closure}}`,
  which is ICF residue, not a call. Consistent with `crates/hydra_ui/NOTES.md`
  ("egui drawing → no drawing at all: model/state/layout logic only"): today
  `hydra_ui` needs **no** egui dependency. Add `egui` only when Phase 4 puts
  drawing back into `view::PlayView`/`story`, and then only for `Ui`/`Painter`/`Color32`.
* `gs_puppet`: **nothing** — it is `std`-only by reconstruction and the binary never
  names an egui type in a `gs_puppet` symbol either.

---

## 4. Dependency recommendation

### 4.1 Root `[workspace.dependencies]`

```toml
# egui family: the binary's exact git rev (see docs/EGUI.md). Every one of these
# must share the same rev, or the types stop unifying.
egui       = { git = "https://github.com/emilk/egui", rev = "3da022d98072fb631443a72f49973cfb6c93f865", default-features = false, features = ["default_fonts"] }
epaint     = { git = "https://github.com/emilk/egui", rev = "3da022d98072fb631443a72f49973cfb6c93f865", default-features = false }
eframe     = { git = "https://github.com/emilk/egui", rev = "3da022d98072fb631443a72f49973cfb6c93f865", default-features = false, features = ["default_fonts", "wgpu"] }
egui-wgpu  = { git = "https://github.com/emilk/egui", rev = "3da022d98072fb631443a72f49973cfb6c93f865", default-features = false, features = ["winit"] }
egui-winit = { git = "https://github.com/emilk/egui", rev = "3da022d98072fb631443a72f49973cfb6c93f865", default-features = false, features = ["clipboard", "links"] }
# optional, only if a crate wants to name them directly; egui re-exports both
ecolor     = { git = "https://github.com/emilk/egui", rev = "3da022d98072fb631443a72f49973cfb6c93f865", default-features = false }
emath      = { git = "https://github.com/emilk/egui", rev = "3da022d98072fb631443a72f49973cfb6c93f865", default-features = false }
```

Notes on the feature choices, each tied to §1.7:

* `wgpu` on eframe also turns on `egui-wgpu/{winit, capture}` (eframe's own dep
  declares them) and `egui-wgpu/default` (→ `wgpu/default`, i.e. the Metal
  backend on macOS). This is exactly what the binary shows.
* `default_fonts` on eframe (→ `egui/default_fonts` → `epaint/default_fonts`):
  the bundled faces are in the image.
* **`accesskit` deliberately omitted**: the binary has no `accesskit_winit` /
  `accesskit_consumer`. Adding it is a one-word change (`"accesskit"`) and is the
  only UX regression of matching the original; it also adds `accesskit_macos`
  and `accesskit_winit 0.32.2` to the offline closure.
* **`persistence` omitted**: no `ron`, no `home`, no `eframe::get_value` in the
  binary.
* **`glow` omitted**: no `egui_glow` anywhere.
* `x11`/`wayland` are in eframe's *default* feature set but are no-ops on macOS;
  the binary cannot prove or disprove them. If Linux must keep building, add
  `"x11", "wayland"` to eframe (and `egui-wgpu`, `egui-winit` inherit through
  eframe). Nothing in the evidence opposes it.
* `egui-winit` is only needed directly if a crate names `egui_winit::State`
  (nothing does today); it is listed because eframe pulls it with exactly these
  features, and stating them keeps the intent explicit.

### 4.2 Per-crate lines

```toml
# crates/hydra_gui/Cargo.toml
[dependencies]
eframe = { workspace = true }
egui = { workspace = true }
epaint = { workspace = true }          # or use egui::epaint and drop this line
hydra_board = { path = "../hydra_board" }
hydra_ui = { path = "../hydra_ui" }
hydra_session = { path = "../hydra_session" }
hydra_model = { path = "../hydra_model" }
gs_base = { path = "../gs_base" }
image = { workspace = true }           # badge::pixels (load_from_memory/resize_exact/to_rgba8)

# crates/gs_render/Cargo.toml
[dependencies]
egui = { workspace = true }
epaint = { workspace = true }
egui-wgpu = { workspace = true }
wgpu = { workspace = true }
gs_base = { path = "../gs_base" }
gs_puppet = { path = "../gs_puppet" }

# crates/hydra_board/Cargo.toml
[dependencies]
egui = { workspace = true }            # Ui::available_width
epaint = { workspace = true }          # TextureHandle in disk::Disk::kept_at

# crates/hydra_ui/Cargo.toml, crates/gs_puppet/Cargo.toml
#   nothing (see §3.6)
```

`eframe` re-exports `egui`, `egui::emath`, `egui::epaint`, `egui_wgpu` and
`egui_wgpu::wgpu`, so the extra `egui`/`epaint`/`wgpu` lines above are for clarity and
for crates that do not depend on eframe (`gs_render`, `hydra_board`), not because
cargo needs them twice.

`hydra_gui`'s existing stubs (`crate::seam`, `crate::json`, `badge.rgba`) become
dead weight once this lands; that cleanup belongs to the Phase 4 job that adds the
code, not here.

### 4.3 Compatibility with the workspace's strict pins

| Hydra pin | what egui wants | verdict |
|---|---|---|
| `wgpu = "=29.0.4"` | `wgpu = "29.0"`, `features = ["std"]` (egui-wgpu adds `default`) | ✓ resolves to 29.0.4 |
| `winit = "=0.30.13"` | `winit = "0.30.13"` | ✓ identical |
| `bytemuck = "=1.25.2"` | `1.25` | ✓ |
| `image = "=0.25.10"` | eframe: `image = "0.25.6"` + `png` | ✓ |
| `serde = "=1.0.229"` | `1.0` (only with `persistence`/`serde`, which are off) | ✓ |
| `parking_lot = "=0.12.5"`, `smallvec = "=1.16.1"`, `ahash`, `jiff` | `0.12.5`, `1.15`, `0.8.12`, `0.2` | ✓ |
| `toml = "=1.1.6"` + `toml_edit` | egui's `toml = "1.0"` is dev-only in its workspace | ✓ (and the binary already ships both `toml 1.1.6` and `toml_edit 0.22.27`, exactly the duplicate the rev's own `Cargo.toml` comment predicts) |
| `rustc` | the rev needs `rust-version = "1.92"`, `edition 2024` | ✓ local toolchain is 1.95.0 |

No `[patch]`, no `[replace]`, no `=`-pin conflicts.

### 4.4 Offline / vendoring constraints — the real risk

`cargo --offline` cannot fetch a git dependency, and the egui closure is *not*
cached on this machine:

* `~/.cargo/git` does not exist at all (no `db/`, no `checkouts/`) — the very
  directory the binary was built from is absent;
* `~/.cargo/registry/cache` holds 532 `.crate` files covering today's pins, but of
  the 182 crates in `crate_versions.txt` **68 are missing**, and they include every
  egui-only leaf: `accesskit-0.24.1`, `arboard-3.6.1`, `font-types-0.11.3`,
  `harfrust-0.7.0`, `kurbo-0.13.1`, `objc2-0.5.2`, `objc2-0.6.4`,
  `objc2-app-kit-0.3.2`, `pollster-0.4.0`, `read-fonts-0.39.2`, `skrifa-0.42.1`,
  `unicode-general-category-1.1.0`, `vello_common-0.0.9`, `vello_cpu-0.0.9`,
  `webbrowser-1.2.4` (plus `ahash`/`nohash-hasher`/`self_cell`/`itertools`/
  `profiling`/`raw-window-handle` in the full closure, which `crate_versions.txt`
  does not list because they generate no path strings).

So the Phase-4 job that adds these lines must do **one online step first**, then
go back to `--offline`:

```bash
# once, with network (writes ~/.cargo/git/{db,checkouts}/egui-5b6373fea415a27f/…)
cargo fetch                      # after editing the manifests
ls ~/.cargo/git/checkouts/       # expect: egui-5b6373fea415a27f/3da022d/…
# afterwards, as usual:
cargo check/test --workspace --offline
```

Alternatives if the machine must stay offline:

* `cargo vendor vendor/` (with network, once) plus a `.cargo/config.toml`
  `[source.crates-io] replace-with = "vendored-sources"`; `cargo vendor` copies git
  dependencies too, and everything then builds `--offline`. Cost: tens of MB in
  the repo (egui alone is ~20 MB of sources) and a stale vendor tree to maintain.
* A checked-out source tree plus `[patch."https://github.com/emilk/egui"]` pointing
  at a local path — but the `rev` semantics are lost, which defeats the point.

### 4.5 Risks / unknowns

1. **URL ident hash unverified offline** (§2.5). Cheap to settle (§5.1). If the
   hash differs, only the `git =` URL changes; the `rev` does not.
2. **A branch, not a release.** `3da022d` is on the author's branch of
   `emilk/egui`; a shallow fetch by sha works (`git fetch --depth 1 origin <sha>`),
   but nothing guarantees the server keeps it. If it ever disappears, the fallback
   is a fork or a vendored tarball of the rev, still labelled `0.35.0`.
3. **`x11`/`wayland`** are unprovable from a macOS binary; deciding to omit them
   only affects Linux builds.
4. **`accesskit` off** means no screen-reader tree from the windowing layer (egui's
   internal `accesskit` nodes are still built, since `accesskit` is a hard
   dependency of `egui` at this rev).
5. **`epaint`'s text stack is fragile by construction**: the rev's `Cargo.toml`
   comments pin `harfrust 0.7`, `skrifa 0.42.1`, `font-types 0.11.3`,
   `vello_cpu 0.0.9` because `vello_cpu`'s `glifo` pins `read-fonts 0.39`/
   `font-types 0.11`. Do not "helpfully" bump any of them; two versions of
   `font-types` in one graph is a link error waiting to happen.
6. **The `App::ui` trait shape** means `hydra_gui::app::App` cannot be ported to a
   released eframe without rewriting the entry point — this is the main reason the
   git pin is not optional.

---

## 5. How to validate the choice (no code added in this job)

### 5.1 Does cargo agree that the URL is `emilk/egui`?

After adding the lines of §4.1 and running `cargo fetch` **once** with network:

```bash
ls ~/.cargo/git/db/ ~/.cargo/git/checkouts/
#   expect: egui-5b6373fea415a27f   (the ident hash the binary was built with)
#           …/egui-5b6373fea415a27f/3da022d/…   (the exact checkout path)
```

Matching both halves is a full confirmation of §1.6 and §2.5. A different ident
directory means the dependency is a fork/mirror at the same rev.

### 5.2 Symbol probes against the built binary

```bash
nm -a target/release/hydra | rustfilt > /tmp/ours.txt
# 1. exact path string, i.e. the checkout identity
strings -a target/release/hydra | grep -c 'git/checkouts/egui-5b6373fea415a27f/3da022d'
# 2. the branch-only API must be present (they do not exist in any release)
grep -c 'Context::set_viewport_zoom_factor'        /tmp/ours.txt   # ≥1
grep -c 'Context::viewport_zoom_factor'            /tmp/ours.txt   # ≥1
grep -c 'Frame::set_key_capture'                   /tmp/ours.txt   # ≥1
grep -c 'App as eframe::epi::App>::ui'             /tmp/ours.txt   # 1
# 3. the feature verdicts
grep -c 'accesskit_winit'  /tmp/ours.txt   # 0  (accesskit off)
grep -c 'ron::'  /tmp/ours.txt             # 0  (persistence off)
grep -c 'egui_glow' /tmp/ours.txt          # 0  (glow off)
grep -c 'arboard::' /tmp/ours.txt          # >0 (clipboard on)
# 4. rough scale check (the reference binary has 393 global names / 2166 total lines
#    naming an egui crate; expect the same order of magnitude once Phase 4 lands)
```

### 5.3 Type-layout probes (`cargo test`, tiny `#[test]`s)

```rust
assert_eq!(std::mem::size_of::<egui::Color32>(), 4);          // r,g,b,a memory order
assert_eq!(std::mem::size_of::<egui::Vec2>(), 8);
assert_eq!(std::mem::size_of::<egui::Pos2>(), 8);
assert_eq!(std::mem::size_of::<egui::Rect>(), 16);
assert_eq!(std::mem::size_of::<egui::Event>(), 72);           // proved by Keys::take's stride
assert_eq!(egui::Key::H as u8, 53);                           // proved by the name table
assert_eq!(egui::Key::I as u8, 54);
assert_eq!(egui::Key::IntlBackslash as u8, 116);              // 117 variants, no more
assert_eq!(egui::Key::ALL.len(), 117);
```

`egui::Key::ALL` / the `Debug` name list is the cheapest structural check that the
pin is this rev: 117 entries, `ArrowDown` first, `IntlBackslash` last.

### 5.4 A minimal eframe + wgpu smoke test (write it in Phase 4, not here)

```rust
// tests/wgpu_smoke.rs — proves the three integration seams at once:
//   eframe::App::ui, wgpu rendering, and the egui-wgpu paint-callback path
//   gs_render::field_gpu uses.
use eframe::{egui, epaint};

struct Smoke {
    cb: Option<epaint::PaintCallback>,
}

impl eframe::App for Smoke {                     // = eframe::epi::App
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("hydra");
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, egui::Color32::BLACK);
        if let Some(cb) = self.cb.take() {
            ui.painter().add(cb);                // Shape::Callback -> egui_wgpu::Renderer
        }
    }
}

#[test]
fn runs_headless() {
    let mut opts = eframe::NativeOptions::default();
    opts.renderer = eframe::Renderer::Wgpu;      // the only variant when `glow` is off
    let r = eframe::run_native(
        "smoke",
        opts,
        Box::new(|_cc| Ok(Box::new(Smoke { cb: None }) as Box<dyn eframe::App>)),
    );
    assert!(r.is_ok());
}
```

(On macOS a windowed run is fine; for CI prefer `egui_kittest`-free headless checks
— a `Context::default().run_ui(...)`-style test that calls `Bar::fitted` against a
real `Ui` is the cheapest way to prove the `available_size`/`TextStyle::resolve`
seam without a GPU.)

---

## Appendix: what to strike from the crate notes when Phase 4 lands

* `crates/hydra_gui/NOTES.md` → "Departures (egui/eframe/serde/image → std)" items
  1 (`egui` → seam), 3 (`eframe` → `RunPlan`) and 5 (`image` → pre-decoded asset,
  once `image` is reconnected) are exactly what this document replaces.
* `crates/gs_render/NOTES.md` → the `egui`/`egui_wgpu` half of the "Standard-library
  departure" section (`gpu::Gpu`/`NullGpu` seam) is replaced by the
  `CallbackTrait` impl of §3.4.
* `crates/hydra_board/NOTES.md` → the `hydra_ui::story::Story` row only; the
  `epaint::TextureHandle` cache seam (`disk::Disk::kept_at`) is unchanged.
* `crates/hydra_ui/NOTES.md` → the "egui drawing" row stays true until drawing
  returns to `hydra_ui` (§3.6: no egui call edges survive).

---

## 6. Phase 4 outcome (addendum, 2026-10-27)

The §4.1 lines are in the root `Cargo.toml` and all five GUI crates build and
test `cargo --offline` against them. The fetch materialised the checkout at
`~/.cargo/git/checkouts/egui-9c90d2f9798966b7/3da022d`, **not** the
`egui-5b6373fea415a27f` that §5.1 predicts for `https://github.com/emilk/egui`.
Two facts hold independently:

* the rev is the one §2.1 identified — `Context::{set_viewport_zoom_factor,
  viewport_zoom_factor}`, `Frame::{set_key_capture, set_numpad_capture_keys}`
  and `epi::App::ui` all exist in the checkout, the workspace's `version` is
  `0.35.0`, and every API discriminator of §2.4 is present;
* the ident hash differs from the binary's. Either the reference build used a
  fork/mirror at the same rev, or cargo's checkout hash changed between the two
  toolchains (this machine has cargo 1.95.0, 2026-03-21; the reference build is
  from 2026-10). Nothing in the recovered API surface discriminates the two, so
  the pin stays on `emilk/egui` at `3da022d`.

Everything else in this document was confirmed by building: version label
`0.35.0`, wgpu-only renderer, `default_fonts` on, `accesskit`/`persistence`/`glow`
off. Phase 4's evidence tables (§3) are what the crate NOTES.md updates cite.

