# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Lucky is a Tauri v2 desktop application with a React + TypeScript frontend and a Rust backend. Package manager is **bun**.

## Commands

- `bun run tauri dev` — Start the app in development mode (launches both Vite dev server and Tauri window)
- `bun run tauri build` — Build the production app bundle
- `bun run dev` — Start only the Vite frontend dev server (port 1420)
- `bun run build` — TypeScript check + Vite build (frontend only)
- `cd src-tauri && cargo build` — Build only the Rust backend
- `cd src-tauri && cargo test` — Run Rust tests

## Architecture

- **Frontend** (`src/`): React 19 + TypeScript, bundled with Vite. Entry point is `src/main.tsx` → `src/App.tsx`.
- **Backend** (`src-tauri/`): Rust, using Tauri v2. Entry point is `src-tauri/src/main.rs` which calls `lucky_lib::run()` defined in `src-tauri/src/lib.rs`.
- **IPC**: Frontend calls Rust functions via `invoke()` from `@tauri-apps/api/core`. Rust commands are registered in `lib.rs` with `#[tauri::command]` and wired up in `invoke_handler`.
- **Tauri config**: `src-tauri/tauri.conf.json` — app identifier is `com.casperLHK.lucky`, dev server at `localhost:1420`.
- **Capabilities**: `src-tauri/capabilities/default.json` defines permissions for the main window (`core:default`, `opener:default`).
