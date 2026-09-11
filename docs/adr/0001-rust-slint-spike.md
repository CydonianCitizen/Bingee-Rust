# ADR-0001: Evaluate Rust + Slint for Bingee Desktop

- Status: Accepted for spike only
- Date: 2026-09-11

## Context

Bingee Desktop is intended to be a desktop-first, local-first application for Windows, macOS, and Linux. Responsiveness and restrained RAM/CPU use are explicit product goals. Bingee Desktop is not intended to be a direct port of the Android UI.

Two technology spikes will be compared:

- Rust + Slint;
- C# + Avalonia.

## Decision

Build a bounded Rust + Slint spike on `spike/rust-slint`.

This decision authorizes an evaluation only. It does not select Rust + Slint as the final production stack.

## Constraints

- Safe Rust by default.
- Cross-platform design.
- Native Slint UI; no WebView stack.
- Same representative benchmark workload as Avalonia.
- No full-product feature development until the spike comparison is complete.

## Licensing note

Slint licensing must be chosen explicitly before production distribution. The spike must retain required attribution and must not silently assume a proprietary/commercial licensing path.

## Validation

Use `BENCHMARK_SPEC.md` to compare:

- startup;
- RAM;
- CPU idle;
- interaction latency;
- SQLite behavior;
- packaging;
- development ergonomics and maintainability.

## Revisit trigger

Complete R5 and compare the measured Rust/Slint results against the Avalonia spike.
