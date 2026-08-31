# ADR-0003: Bootstrap world coordinates and chunk geometry

Status: **Accepted**

Date: 2026-08-31

Decision owner: project owner

## Context

Prototype 01 needs deterministic chunk generation, negative-coordinate traversal, and a client-facing terrain representation. The accepted architecture intentionally left numeric coordinate representation and chunk dimensions undecided. An executable bootstrap therefore needs an explicit narrow decision without presenting an unmeasured chunk size as a final performance result.

## Decision

For the Prototype 01 bootstrap:

- authoritative terrain positions use integer world-cell coordinates with signed 64-bit axes;
- chunk coordinates use signed 64-bit axes;
- world-cell to chunk/local conversion uses Euclidean division and remainder, including for negative coordinates;
- local cell coordinates are non-negative and bounded by the chunk side;
- world-generation version 1 uses square chunks with a side of 32 cells;
- conversions that exceed the signed world-cell range fail explicitly instead of wrapping.

The 32-cell side is a bootstrap implementation parameter for deterministic tests and diagnostic rendering. It is not a benchmark-selected final chunk size.

Changing chunk geometry after persisted worlds exist requires either:

1. a new world-generation version that preserves the old generator for compatible saves; or
2. an explicit save migration.

This ADR does not select continuous, floating-point, or fixed-point representation for later sub-cell character movement.

## Consequences

The representation covers positive and negative coordinates far beyond practical Prototype 01 traversal while keeping indexing and client read models simple. Euclidean conversion prevents the common error where world cell `-1` is placed in chunk `0` with a negative local coordinate.

The bootstrap may reveal through measurement that another chunk size is preferable. Such a change remains possible before persistence and must be versioned after persistence exists.
