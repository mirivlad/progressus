# ADR-0019 — Primary low-poly client

Status: **Accepted**

Date: 2026-09-10

Decision owner: project owner, who accepted the experiment and explicitly requested replacing the old client on main.

## Decision

The low-poly 3D client replaces the 2D sprite client. There is one runtime and executable. Existing menus, inspectors, commands, save slots, localization and audio settings are retained.

This supersedes ADR-0005's specific raster/canvas world-rendering implementation and terrain alpha-corner technique. Procedural assets remain source-controlled, deterministic, presentation-only Rust recipes; the primary world assets are meshes. A small bitmap icon remains appropriate inside the workstation modal.

Known terrain may derive raised faceted rock ridges, beveled corners and sandy shorelines. Unknown cells emit no geometry and only already published terrain is consulted. Visual edges at the discovery boundary do not claim to describe hidden terrain. Mesh caches are bounded by the viewport and bounded variant/connectivity keys.

ADR-0011's cardinal connection mask is preserved for walls, doors and construction sites. Door orientation follows the predominant adjacent wall axis (east/west on a tie); geometry and height do not change authoritative occupancy, passability or automatic door state. Workstation port direction controls retain their existing authoritative command semantics.

Authoritative simulation coordinates, saves, world generation and stable IDs remain unchanged. Ground-plane picking maps 3D X/-Z to simulation X/Y. Screen-space pawn picking and projected quantity labels provide legibility without affecting authority.

## Consequences

The old world sprite pipeline and separate experiment executable are removed. Standard Linux launch checks and installs missing ALSA build dependencies before invoking Cargo. A working Rust toolchain and graphics driver remain host prerequisites.
