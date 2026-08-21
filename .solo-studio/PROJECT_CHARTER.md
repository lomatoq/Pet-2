# Project Charter — Pet 2

Updated: 2026-08-21
Current gate: Vertical slice

## Non-negotiable promise

> One persistent artificial creature keeps the same genome, body, voice, memories,
> and learned state when moved between Windows and macOS.

## Player and product

- Audience: people who want a quiet, curious desktop companion.
- Platform: Windows 10/11 x64 first; macOS 13+ Apple Silicon second.
- Business model: unspecified; runtime is offline and service-free.
- Session length: ambient, potentially all day.
- Target hardware: ordinary D3D12 Windows PCs and Apple Silicon Macs.

## Product identity

- One-sentence pitch: a procedural desktop creature that is a persistent organism,
  not a skinned widget.
- Identity pillars: continuity, procedural embodiment, low-interference presence.
- Must not become: separate platform forks, a full-screen overlay, or an asset-heavy
  sprite application.
- Visual thesis: a small readable procedural silhouette on a fully transparent canvas.
- Audio/motion thesis: restrained signals generated from the same genome as the body.

## Current milestone

- Decisive uncertainty: can one portable domain/state contract survive native window,
  monitor, persistence, renderer, and audio boundaries without OS types leaking inward?
- Evidence required: workspace builds; deterministic hashes; state fixture round-trips;
  coordinate and offline-audio matrix tests; Windows and macOS CI jobs.
- Golden path: create seeded pet -> simulate headlessly -> save -> load -> reproduce
  genome/body/voice hashes -> launch a small transparent overlay.
- Explicit cut line: no services, editor, content pipeline, or asset-dependent art pass.

## Technical envelope

- Engine: Rust 2024, winit 0.30.12, wgpu 24.0.5, cpal 0.15.3.
- Target FPS: 60 active / 15 sleeping.
- Memory target: <150 MB; release package target: <50 MB.
- Save: versioned JSON, normalized position, atomic replacement plus backup.
- Network/service requirements: none.
- Required fallback: reduced-capability portable host; silent operation after audio loss.
- Unsupported: Linux is compile-capable only until X11 and Wayland are tested.

## Decision rules

1. Keep simulation and portable state independent of native platform handles.
2. Prefer capability absence over fabricated sensor data or process failure.
3. Preserve the cross-platform organism before adding presentation breadth.
