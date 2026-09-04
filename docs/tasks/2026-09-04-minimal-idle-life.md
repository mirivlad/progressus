# Task: minimal idle life

Status: **Implemented; owner-PC visual validation pending**
Date: 2026-09-04

## Goal

Make the five-person settlement look inhabited when no explicit job or urgent material need is active, without introducing mood, recreation, relationships or a speculative general AI system.

## Implemented behavior

- authoritative `Wandering` movement distinct from player navigation;
- deterministic staggered idle decisions with long standing pauses between them;
- short wandering routes only through already explored walkable cells;
- fixed local idle anchor with Manhattan radius 3 so repeated wandering cannot drift across the world;
- rare approach-to-nearby-idle-person behavior inside the same bound;
- ordinary jobs treat wandering people as available workers and preempt the idle route;
- hunger/starvation can interrupt idle movement;
- player movement commands retain direct priority;
- wandering and idle anchors survive save/load deterministically;
- selected-character UI labels the state as `гуляет` / `wandering`.

## Non-goals

- mood or mental breaks;
- recreation need/bar;
- friendships or relationships;
- conversations with simulated content;
- personality-driven schedules;
- autonomous exploration;
- homes, gathering spots or leisure buildings before their milestone systems exist.

## Validation

Headless tests cover deterministic bounded wandering, stable anchors, work preemption and save/load continuation. The graphical client is not built on `tomas`; owner-PC validation is required after pulling.
